use std::net::SocketAddr;

use anyhow::Context;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod bootstrap_admin;
mod domain;
mod error;
mod routes;
mod services;
mod state;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    bootstrap_admin::ensure_from_env(&config.database_url).await?;
    let _otel_provider = init_tracing(&config)?;
    let state = AppState::new(config.clone()).await?;

    if !config.process_role.serves_http() {
        tracing::info!(
            process_role = ?config.process_role,
            "zoohelp backend worker process started"
        );
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for shutdown signal")?;
        tracing::info!("zoohelp backend worker process shutting down");
        return Ok(());
    }

    let app = routes::router(state)
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(cors_layer(&config)?)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = config.bind_addr.parse().context("invalid BIND_ADDR")?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, process_role = ?config.process_role, "zoohelp rust backend listening");

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing(config: &Config) -> anyhow::Result<Option<SdkTracerProvider>> {
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let fmt_layer = tracing_subscriber::fmt::layer().json();

    if let Some(endpoint) = config.otel_exporter_otlp_endpoint.as_deref() {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.to_string())
            .build()
            .context("failed to build OTLP trace exporter")?;
        let provider = SdkTracerProvider::builder()
            .with_resource(
                Resource::builder()
                    .with_service_name("zoohelp-backend")
                    .build(),
            )
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("zoohelp-backend");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();

        Ok(Some(provider))
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(None)
    }
}

fn cors_layer(config: &Config) -> anyhow::Result<CorsLayer> {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ];

    if config.is_development() && config.cors_allowed_origins.is_empty() {
        return Ok(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(methods)
            .allow_headers(Any));
    }

    let origins = config
        .cors_allowed_origins
        .iter()
        .map(|origin| origin.parse::<HeaderValue>())
        .collect::<Result<Vec<_>, _>>()
        .context("invalid CORS_ALLOWED_ORIGINS")?;

    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(Any))
}
