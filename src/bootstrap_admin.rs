use anyhow::Context;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

/// Creates or updates a single administrator when both bootstrap variables are
/// present. The password is deliberately supplied only at runtime by Railway;
/// it must never be committed to the repository.
pub async fn ensure_from_env(database_url: &str) -> anyhow::Result<()> {
    let email = std::env::var("BOOTSTRAP_ADMIN_EMAIL").ok();
    let password = std::env::var("BOOTSTRAP_ADMIN_PASSWORD").ok();

    match (email, password) {
        (None, None) => Ok(()),
        (Some(email), Some(password)) => {
            let email = email.trim().to_ascii_lowercase();
            anyhow::ensure!(!email.is_empty(), "BOOTSTRAP_ADMIN_EMAIL must not be empty");
            anyhow::ensure!(password.len() >= 12, "BOOTSTRAP_ADMIN_PASSWORD must have at least 12 characters");

            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect(database_url)
                .await
                .context("failed to connect while bootstrapping administrator")?;
            let name = std::env::var("BOOTSTRAP_ADMIN_NAME")
                .unwrap_or_else(|_| "Paulo Admin".to_string());
            upsert_admin(&pool, &email, &password, name.trim()).await?;
            tracing::info!(email, "bootstrap administrator ensured");
            Ok(())
        }
        _ => anyhow::bail!(
            "BOOTSTRAP_ADMIN_EMAIL and BOOTSTRAP_ADMIN_PASSWORD must be configured together"
        ),
    }
}

async fn upsert_admin(pool: &PgPool, email: &str, password: &str, name: &str) -> anyhow::Result<()> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("administrator password hashing failed: {error}"))?;

    sqlx::query(
        r#"
        INSERT INTO users (name, email, password_hash, account_type, verified, trust_score)
        VALUES ($1, $2, $3, 'admin'::account_type, true, 100)
        ON CONFLICT (email) DO UPDATE SET
          name = EXCLUDED.name,
          password_hash = EXCLUDED.password_hash,
          account_type = 'admin'::account_type,
          verified = true,
          trust_score = GREATEST(users.trust_score, 100),
          deleted_at = NULL,
          anonymized_at = NULL,
          retention_delete_after = NULL
        "#,
    )
    .bind(name)
    .bind(email)
    .bind(password_hash)
    .execute(pool)
    .await
    .context("failed to upsert bootstrap administrator")?;

    Ok(())
}
