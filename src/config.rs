use std::env;

use anyhow::Context;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessRole {
    All,
    Api,
    Workers,
    PushWorker,
    GeocodeWorker,
    FanoutWorker,
}

impl ProcessRole {
    pub fn from_env_value(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "all" => Ok(Self::All),
            "api" | "web" | "http" => Ok(Self::Api),
            "workers" | "worker" => Ok(Self::Workers),
            "push-worker" | "push_worker" | "push" => Ok(Self::PushWorker),
            "geocode-worker" | "geocode_worker" | "geocode" | "geocoding" => {
                Ok(Self::GeocodeWorker)
            }
            "fanout-worker" | "fanout_worker" | "fanout" => Ok(Self::FanoutWorker),
            other => anyhow::bail!(
                "PROCESS_ROLE must be all, api, workers, push-worker, geocode-worker, or fanout-worker; got {other}"
            ),
        }
    }

    pub fn serves_http(self) -> bool {
        matches!(self, Self::All | Self::Api)
    }

    pub fn starts_push_worker(self) -> bool {
        matches!(self, Self::All | Self::Workers | Self::PushWorker)
    }

    pub fn starts_geocode_worker(self) -> bool {
        matches!(self, Self::All | Self::Workers | Self::GeocodeWorker)
    }

    pub fn starts_fanout_worker(self) -> bool {
        matches!(self, Self::All | Self::Workers | Self::FanoutWorker)
    }

    pub fn starts_realtime_bridge(self) -> bool {
        self.serves_http()
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub app_env: String,
    pub process_role: ProcessRole,
    pub bind_addr: String,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_min_connections: u32,
    pub redis_url: String,
    pub nats_url: String,
    pub ai_worker_url: String,
    pub jwt_secret: String,
    pub cloudinary_cloud_name: String,
    pub cloudinary_api_key: Option<String>,
    pub cloudinary_api_secret: Option<String>,
    pub geocoding_api_provider: Option<String>,
    pub google_maps_api_key: Option<String>,
    #[allow(dead_code)]
    pub api_public_url: String,
    pub app_public_url: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_pass: Option<String>,
    pub smtp_secure: bool,
    pub smtp_from_email: String,
    pub smtp_from_name: String,
    pub access_token_ttl_minutes: i64,
    pub refresh_token_ttl_days: i64,
    pub cors_allowed_origins: Vec<String>,
    pub postgis_enabled: bool,
    pub payments_enabled: bool,
    pub payment_provider: String,
    pub payment_webhook_secret: Option<String>,
    pub sentry_dsn: Option<String>,
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub push_worker_enabled: bool,
    pub rescue_fanout_worker_enabled: bool,
    pub push_provider: String,
    pub expo_access_token: Option<String>,
    pub log_push_tokens: bool,
    pub throttle_ttl_seconds: u64,
    pub throttle_limit: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let config = Self {
            app_env: env::var("APP_ENV")
                .or_else(|_| env::var("RUST_ENV"))
                .unwrap_or_else(|_| "development".to_string()),
            process_role: ProcessRole::from_env_value(
                &env::var("PROCESS_ROLE").unwrap_or_else(|_| "all".to_string()),
            )?,
            bind_addr: env::var("PORT")
                .map(|port| format!("0.0.0.0:{port}"))
                .or_else(|_| env::var("BIND_ADDR"))
                .unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            database_max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(20),
            database_min_connections: env::var("DATABASE_MIN_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            nats_url: env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            ai_worker_url: env::var("AI_WORKER_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-only-change-me-before-production".to_string()),
            cloudinary_cloud_name: env::var("CLOUDINARY_CLOUD_NAME")
                .ok()
                .or_else(|| cloud_name_from_url(&env::var("CLOUDINARY_URL").ok()?))
                .unwrap_or_else(|| "zoohelp-dev".to_string()),
            cloudinary_api_key: env::var("CLOUDINARY_API_KEY").ok(),
            cloudinary_api_secret: env::var("CLOUDINARY_API_SECRET").ok(),
            geocoding_api_provider: env::var("GEOCODING_API_PROVIDER").ok(),
            google_maps_api_key: env::var("GOOGLE_MAPS_API_KEY").ok(),
            api_public_url: env::var("API_PUBLIC_URL")
                .or_else(|_| env::var("EXPO_PUBLIC_API_BASE_URL"))
                .unwrap_or_else(|_| "https://helpin-platform-core-production.up.railway.app".to_string()),
            app_public_url: env::var("APP_PUBLIC_URL")
                .unwrap_or_else(|_| "https://zoohelp.app".to_string()),
            smtp_host: env::var("SMTP_HOST")
                .ok()
                .or_else(|| env::var("MTP_HOST").ok()),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(587),
            smtp_user: env::var("SMTP_USER").ok(),
            smtp_pass: env::var("SMTP_PASS").ok(),
            smtp_secure: env::var("SMTP_SECURE")
                .ok()
                .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
                .unwrap_or(true),
            smtp_from_email: env::var("SMTP_FROM_EMAIL")
                .ok()
                .or_else(|| env::var("SMTP_USER").ok())
                .unwrap_or_else(|| "no-reply@zoohelp.app".to_string()),
            smtp_from_name: env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "Helpin".to_string()),
            access_token_ttl_minutes: env::var("ACCESS_TOKEN_TTL_MINUTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(15),
            refresh_token_ttl_days: env::var("REFRESH_TOKEN_TTL_DAYS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30),
            cors_allowed_origins: env::var("CORS_ALLOWED_ORIGINS")
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            postgis_enabled: env_bool("POSTGIS_ENABLED").unwrap_or(false),
            payments_enabled: env_bool("PAYMENTS_ENABLED").unwrap_or(false),
            payment_provider: env::var("PAYMENT_PROVIDER")
                .unwrap_or_else(|_| "disabled".to_string()),
            payment_webhook_secret: env::var("PAYMENT_WEBHOOK_SECRET").ok(),
            sentry_dsn: env::var("SENTRY_DSN").ok(),
            otel_exporter_otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            push_worker_enabled: env::var("PUSH_WORKER_ENABLED")
                .ok()
                .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
                .unwrap_or(false),
            rescue_fanout_worker_enabled: env::var("RESCUE_FANOUT_WORKER_ENABLED")
                .ok()
                .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
                .unwrap_or_else(|| {
                    env::var("PUSH_WORKER_ENABLED")
                        .ok()
                        .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
                        .unwrap_or(false)
                }),
            push_provider: env::var("PUSH_PROVIDER").unwrap_or_else(|_| "expo".to_string()),
            expo_access_token: env::var("EXPO_ACCESS_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            log_push_tokens: env_bool("LOG_PUSH_TOKENS").unwrap_or(false),
            throttle_ttl_seconds: env::var("THROTTLE_TTL")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(60),
            throttle_limit: env::var("THROTTLE_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
        };

        config.validate()?;
        Ok(config)
    }

    pub fn is_development(&self) -> bool {
        matches!(self.app_env.as_str(), "development" | "dev" | "test")
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.is_development() {
            return Ok(());
        }

        anyhow::ensure!(
            self.jwt_secret != "dev-only-change-me-before-production"
                && self.jwt_secret != "change-this-64-byte-secret-before-production"
                && self.jwt_secret != "replace-with-random-64-byte-production-secret"
                && self.jwt_secret.len() >= 32,
            "JWT_SECRET must be strong outside development"
        );
        anyhow::ensure!(
            (1..=60).contains(&self.access_token_ttl_minutes),
            "ACCESS_TOKEN_TTL_MINUTES must be between 1 and 60 outside development"
        );
        anyhow::ensure!(
            self.cloudinary_api_key.is_some() && self.cloudinary_api_secret.is_some(),
            "Cloudinary credentials are required outside development"
        );
        anyhow::ensure!(
            !self.cors_allowed_origins.is_empty(),
            "CORS_ALLOWED_ORIGINS is required outside development"
        );
        anyhow::ensure!(
            !self.redis_url.trim().is_empty(),
            "REDIS_URL is required outside development"
        );
        anyhow::ensure!(
            !self.nats_url.trim().is_empty(),
            "NATS_URL is required outside development"
        );
        if self.process_role.starts_push_worker() {
            anyhow::ensure!(
                self.push_worker_enabled,
                "PUSH_WORKER_ENABLED=true is required outside development when PROCESS_ROLE starts push delivery"
            );
        }
        if self.process_role.starts_fanout_worker() {
            anyhow::ensure!(
                self.rescue_fanout_worker_enabled,
                "RESCUE_FANOUT_WORKER_ENABLED=true is required outside development when PROCESS_ROLE starts fanout"
            );
        }
        let geocoding_provider = self
            .geocoding_api_provider
            .as_deref()
            .unwrap_or("auto")
            .trim()
            .to_ascii_lowercase();
        anyhow::ensure!(
            matches!(
                geocoding_provider.as_str(),
                "auto" | "google" | "google_maps" | "osm" | "nominatim" | "openstreetmap"
            ),
            "GEOCODING_API_PROVIDER must be auto, google, or osm outside development"
        );
        if self.payments_enabled {
            anyhow::ensure!(
                self.payment_provider != "disabled"
                    && self.payment_provider != "manual_psp_required"
                    && self
                        .payment_webhook_secret
                        .as_deref()
                        .is_some_and(|value| value.len() >= 24),
                "PAYMENT_PROVIDER and a strong PAYMENT_WEBHOOK_SECRET are required when PAYMENTS_ENABLED=true"
            );
        }
        Ok(())
    }
}

fn env_bool(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
}

fn cloud_name_from_url(value: &str) -> Option<String> {
    value
        .rsplit_once('@')
        .map(|(_, cloud_name)| cloud_name.trim().trim_matches('/').to_string())
        .filter(|cloud_name| !cloud_name.is_empty())
}
