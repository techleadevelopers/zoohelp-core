use std::time::Duration as StdDuration;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;
use validator::Validate;

use crate::{
    domain::AccountType,
    error::ApiError,
    services::{auth as auth_service, rate_limit},
    state::AppState,
};

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 6))]
    pub password: String,
}
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    #[validate(length(min = 2, max = 120))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    pub account_type: Option<AccountType>,
    pub gender: Option<String>,
    pub avatar: Option<String>,
    pub ong_type: Option<String>,
    pub cnpj: Option<String>,
    pub phone: Option<String>,
    pub cep: Option<String>,
    pub street: Option<String>,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub neighborhood: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub foundation_year: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub avatar: Option<String>,
    pub bio: String,
    #[serde(rename = "type")]
    pub account_type: AccountType,
    pub verified: bool,
    pub gender: Option<String>,
    pub posts_count: u32,
    pub helped_count: u32,
    pub adoptions_count: u32,
    pub profile_address: Option<ProfileAddress>,
}

#[derive(Default)]
struct UserStats {
    posts_count: u32,
    helped_count: u32,
    adoptions_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub user: UserProfile,
    pub ong_profile: Option<OngRegistrationProfile>,
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUserResponse {
    pub user: UserProfile,
    pub ong_profile: Option<OngRegistrationProfile>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PasswordResetRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPasswordResetRequest {
    #[validate(length(min = 20, max = 120))]
    pub token: String,
    #[validate(length(min = 8, max = 256))]
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailQuery {
    pub token: String,
}

#[derive(Serialize)]
pub struct ActionQueuedResponse {
    pub status: &'static str,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvatarRequest {
    #[validate(url)]
    pub avatar_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvatarResponse {
    pub avatar_url: String,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    #[validate(length(min = 2, max = 120))]
    pub name: String,
    #[validate(length(min = 0, max = 8))]
    pub cep: Option<String>,
    #[validate(length(min = 0, max = 160))]
    pub street: Option<String>,
    #[validate(length(min = 0, max = 20))]
    pub number: Option<String>,
    #[validate(length(min = 0, max = 80))]
    pub complement: Option<String>,
    #[validate(length(min = 0, max = 120))]
    pub neighborhood: Option<String>,
    #[validate(length(min = 0, max = 120))]
    pub city: Option<String>,
    #[validate(length(min = 0, max = 2))]
    pub state: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAddress {
    pub cep: Option<String>,
    pub street: Option<String>,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub neighborhood: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OngRegistrationProfile {
    pub legal_name: String,
    pub ong_type: Option<String>,
    pub cnpj: Option<String>,
    pub phone: Option<String>,
    pub cep: Option<String>,
    pub street: Option<String>,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub neighborhood: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub foundation_year: Option<i32>,
    pub verification_status: String,
}

pub async fn login(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    rate_limit::check_ip(
        &state,
        &headers,
        "auth:login",
        5,
        StdDuration::from_secs(60),
    )
    .await?;
    rate_limit::check_key(
        &state,
        &format!("auth:login:{}", payload.email),
        state.config.throttle_limit,
        StdDuration::from_secs(state.config.throttle_ttl_seconds),
    )
    .await?;

    match find_user_by_email(&state, &payload.email).await {
        Ok(Some(record)) => {
            if !auth_service::verify_password(&payload.password, &record.password_hash) {
                return Err(ApiError::Unauthorized);
            }
            let ong_record = if matches!(record.account_type, AccountType::Ong) {
                find_ong_by_user_id(&state, record.id).await?
            } else {
                None
            };
            issue_auth_response(&state, record, ong_record)
                .await
                .map(Json)
        }
        Ok(None) => Err(ApiError::Unauthorized),
        Err(error) if state.config.is_development() => {
            tracing::warn!(
                ?error,
                "database login path unavailable; using dev fallback"
            );
            let name = payload
                .email
                .split('@')
                .next()
                .unwrap_or("Você")
                .to_string();
            issue_fallback_response(
                &state,
                "me",
                &name,
                &payload.email,
                None,
                AccountType::Person,
                None,
                None,
            )
            .map(Json)
        }
        Err(error) => {
            tracing::error!(?error, "database login path unavailable");
            Err(ApiError::ServiceUnavailable)
        }
    }
}

pub async fn register(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    rate_limit::check_ip(
        &state,
        &headers,
        "auth:register",
        3,
        StdDuration::from_secs(60 * 60),
    )
    .await?;
    rate_limit::check_key(
        &state,
        &format!("auth:register:{}", payload.email),
        state.config.throttle_limit.max(2) / 2,
        StdDuration::from_secs(state.config.throttle_ttl_seconds * 5),
    )
    .await?;

    let account_type = payload.account_type.clone().unwrap_or(AccountType::Person);
    if matches!(account_type, AccountType::Admin) {
        return Err(ApiError::Validation(
            "administrator accounts can only be provisioned by server bootstrap".into(),
        ));
    }
    let password_hash = auth_service::hash_password(&payload.password).map_err(|error| {
        tracing::error!(?error, "password hashing failed");
        ApiError::Internal
    })?;

    validate_ong_payload(&payload, &account_type)?;

    match insert_user_with_optional_ong(&state, &payload, &account_type, &password_hash).await {
        Ok((record, ong_profile)) => issue_auth_response(&state, record, ong_profile)
            .await
            .map(Json),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::Conflict("email already registered".into()))
        }
        Err(error) if state.config.is_development() => {
            tracing::warn!(
                ?error,
                "database register path unavailable; using dev fallback"
            );
            let response = issue_fallback_response(
                &state,
                "me",
                &payload.name,
                &payload.email,
                payload.avatar.as_deref(),
                account_type.clone(),
                normalize_gender(payload.gender.as_deref()),
                fallback_ong_record(&payload, &account_type),
            )?;
            Ok(Json(response))
        }
        Err(error) => {
            tracing::error!(?error, "database register path unavailable");
            Err(ApiError::ServiceUnavailable)
        }
    }
}

pub async fn request_password_reset(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<PasswordResetRequest>,
) -> Result<Json<ActionQueuedResponse>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    rate_limit::check_ip(
        &state,
        &headers,
        "auth:password-reset",
        6,
        StdDuration::from_secs(60 * 60),
    )
    .await?;
    rate_limit::check_key(
        &state,
        &format!(
            "auth:password-reset:{}:{}",
            payload.email.to_lowercase(),
            rate_limit::client_ip(&headers)
        ),
        3,
        StdDuration::from_secs(60 * 60),
    )
    .await?;
    queue_password_reset(&state, &payload.email).await;
    Ok(Json(ActionQueuedResponse { status: "queued" }))
}

pub async fn confirm_password_reset(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<ConfirmPasswordResetRequest>,
) -> Result<Json<ActionQueuedResponse>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    rate_limit::check_ip(
        &state,
        &headers,
        "auth:password-reset-confirm",
        10,
        StdDuration::from_secs(60),
    )
    .await?;
    rate_limit::check_key(
        &state,
        "auth:password-reset-confirm",
        state.config.throttle_limit,
        StdDuration::from_secs(state.config.throttle_ttl_seconds),
    )
    .await?;

    let password_hash = auth_service::hash_password(&payload.password).map_err(|error| {
        tracing::error!(?error, "password reset hashing failed");
        ApiError::Internal
    })?;

    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"
        UPDATE password_reset_tokens
        SET used_at = now()
        WHERE token = $1
          AND used_at IS NULL
          AND expires_at > now()
        RETURNING user_id
        "#,
    )
    .bind(payload.token.trim())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::Unauthorized);
    };
    let user_id: Uuid = row.get("user_id");

    sqlx::query(
        r#"
        UPDATE users
        SET password_hash = $1
        WHERE id = $2
          AND deleted_at IS NULL
        "#,
    )
    .bind(&password_hash)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    audit_event(
        &state,
        Some(user_id),
        "auth.password_reset.confirmed",
        serde_json::json!({}),
    )
    .await;

    Ok(Json(ActionQueuedResponse { status: "updated" }))
}

pub async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> Result<Json<ActionQueuedResponse>, ApiError> {
    if query.token.trim().is_empty() {
        return Err(ApiError::Validation("token is required".into()));
    }

    let result = sqlx::query(
        r#"
        UPDATE users
        SET verified = true
        WHERE id = (
          SELECT user_id
          FROM email_verification_tokens
          WHERE token = $1
            AND used_at IS NULL
            AND expires_at > now()
        )
        RETURNING id
        "#,
    )
    .bind(&query.token)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(row)) => {
            let user_id: Uuid = row.get("id");
            let _ = sqlx::query(
                "UPDATE email_verification_tokens SET used_at = now() WHERE token = $1",
            )
            .bind(&query.token)
            .execute(&state.db)
            .await;
            tracing::info!(%user_id, "email verified");
            Ok(Json(ActionQueuedResponse { status: "verified" }))
        }
        Ok(None) => Err(ApiError::NotFound),
        Err(error) => {
            tracing::error!(?error, "email verification failed");
            Err(ApiError::Internal)
        }
    }
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CurrentUserResponse>, ApiError> {
    let claims = authenticate_request(&state, &headers)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;
    let record = find_user_by_id(&state, user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let ong_record = if matches!(record.account_type, AccountType::Ong) {
        find_ong_by_user_id(&state, record.id).await?
    } else {
        None
    };

    Ok(Json(
        current_user_response(&state, record, ong_record).await,
    ))
}

pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ActionQueuedResponse>, ApiError> {
    let claims = authenticate_request(&state, &headers)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;
    let anonymized_email = format!("deleted+{}@zoohelp.local", user_id);

    let result = sqlx::query(
        r#"
        UPDATE users
        SET
          name = 'Deleted user',
          email = $2,
          avatar_url = NULL,
          verified = false,
          deleted_at = now(),
          anonymized_at = now(),
          retention_delete_after = now() + interval '90 days'
        WHERE id = $1
          AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(anonymized_email)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    let _ = sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await;

    audit_event(
        &state,
        Some(user_id),
        "user.account.deleted",
        serde_json::json!({ "retentionDays": 90 }),
    )
    .await;

    Ok(Json(ActionQueuedResponse { status: "deleted" }))
}

pub async fn update_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateAvatarRequest>,
) -> Result<Json<UpdateAvatarResponse>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    let claims = authenticate_request(&state, &headers)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;
    rate_limit::check_key(
        &state,
        &format!("profile:avatar:{user_id}"),
        state.config.throttle_limit,
        StdDuration::from_secs(state.config.throttle_ttl_seconds),
    )
    .await?;
    validate_owned_avatar_url(&state, user_id, &payload.avatar_url).await?;

    sqlx::query("UPDATE users SET avatar_url = $1 WHERE id = $2")
        .bind(&payload.avatar_url)
        .bind(user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(UpdateAvatarResponse {
        avatar_url: payload.avatar_url,
    }))
}

pub async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<CurrentUserResponse>, ApiError> {
    payload
        .validate()
        .map_err(|error| ApiError::Validation(error.to_string()))?;

    let state_code = normalize_optional(payload.state.as_deref()).map(|value| value.to_uppercase());
    if state_code
        .as_deref()
        .is_some_and(|value| value.chars().count() != 2)
    {
        return Err(ApiError::Validation("state must be a 2-letter UF".into()));
    }

    let claims = authenticate_request(&state, &headers)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;
    rate_limit::check_key(
        &state,
        &format!("profile:update:{user_id}"),
        state.config.throttle_limit,
        StdDuration::from_secs(state.config.throttle_ttl_seconds),
    )
    .await?;
    let name = payload.name.trim().to_string();
    let cep = normalize_optional(payload.cep.as_deref());
    if cep
        .as_deref()
        .is_some_and(|value| value.len() != 8 || !value.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Err(ApiError::Validation("cep must contain 8 digits".into()));
    }
    let street = normalize_optional(payload.street.as_deref());
    let number = normalize_optional(payload.number.as_deref());
    let complement = normalize_optional(payload.complement.as_deref());
    let neighborhood = normalize_optional(payload.neighborhood.as_deref());
    let city = normalize_optional(payload.city.as_deref());
    if city.is_some() ^ state_code.is_some() {
        return Err(ApiError::Validation(
            "city and state must be sent together".into(),
        ));
    }

    let mut tx = state.db.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE users
        SET name = $2, cep = $3, street = $4, number = $5, complement = $6,
            neighborhood = $7, city = $8, state = $9
        WHERE id = $1
          AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(&name)
    .bind(cep.as_deref())
    .bind(street.as_deref())
    .bind(number.as_deref())
    .bind(complement.as_deref())
    .bind(neighborhood.as_deref())
    .bind(city.as_deref())
    .bind(state_code.as_deref())
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    sqlx::query(
        r#"
        UPDATE ong_profiles
        SET legal_name = $2, cep = $3, street = $4, number = $5, complement = $6,
            neighborhood = $7, city = $8, state = $9
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(&name)
    .bind(cep.as_deref())
    .bind(street.as_deref())
    .bind(number.as_deref())
    .bind(complement.as_deref())
    .bind(neighborhood.as_deref())
    .bind(city.as_deref())
    .bind(state_code.as_deref())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let record = find_user_by_id(&state, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let ong_record = if matches!(record.account_type, AccountType::Ong) {
        find_ong_by_user_id(&state, user_id).await?
    } else {
        None
    };

    Ok(Json(
        current_user_response(&state, record, ong_record).await,
    ))
}

#[derive(Debug)]
struct UserRecord {
    id: Uuid,
    name: String,
    email: String,
    avatar: Option<String>,
    password_hash: String,
    account_type: AccountType,
    verified: bool,
    gender: Option<String>,
    cep: Option<String>,
    street: Option<String>,
    number: Option<String>,
    complement: Option<String>,
    neighborhood: Option<String>,
    city: Option<String>,
    state: Option<String>,
}

#[derive(Debug)]
struct OngRecord {
    legal_name: String,
    ong_type: Option<String>,
    cnpj: Option<String>,
    phone: Option<String>,
    cep: Option<String>,
    street: Option<String>,
    number: Option<String>,
    complement: Option<String>,
    neighborhood: Option<String>,
    city: Option<String>,
    state: Option<String>,
    foundation_year: Option<i32>,
    verification_status: String,
}

async fn find_user_by_email(
    state: &AppState,
    email: &str,
) -> Result<Option<UserRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT id, name, email, avatar_url, password_hash, account_type::text AS account_type, verified, gender,
               cep, street, number, complement, neighborhood, city, state
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.map(row_to_user_record))
}

async fn find_user_by_id(state: &AppState, id: Uuid) -> Result<Option<UserRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT id, name, email, avatar_url, password_hash, account_type::text AS account_type, verified, gender,
               cep, street, number, complement, neighborhood, city, state
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.map(row_to_user_record))
}

async fn insert_user_with_optional_ong(
    state: &AppState,
    payload: &RegisterRequest,
    account_type: &AccountType,
    password_hash: &str,
) -> Result<(UserRecord, Option<OngRecord>), sqlx::Error> {
    let account_type_str = auth_service::account_type_as_str(account_type);
    let mut tx = state.db.begin().await?;
    let user_id = Uuid::now_v7();
    let row = sqlx::query(
        r#"
        INSERT INTO users (
          id, name, email, avatar_url, password_hash, account_type, verified, gender,
          cep, street, number, complement, neighborhood, city, state
        )
        VALUES ($1, $2, $3, $4, $5, $6::account_type, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING id, name, email, avatar_url, password_hash, account_type::text AS account_type, verified, gender,
                  cep, street, number, complement, neighborhood, city, state
        "#,
    )
    .bind(user_id)
    .bind(&payload.name)
    .bind(&payload.email)
    .bind(payload.avatar.as_deref())
    .bind(password_hash)
    .bind(account_type_str)
    .bind(matches!(account_type, AccountType::Person | AccountType::Vet | AccountType::Admin))
    .bind(normalize_gender(payload.gender.as_deref()))
    .bind(payload.cep.as_deref())
    .bind(payload.street.as_deref())
    .bind(payload.number.as_deref())
    .bind(payload.complement.as_deref())
    .bind(payload.neighborhood.as_deref())
    .bind(payload.city.as_deref())
    .bind(payload.state.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    let ong_record = if matches!(account_type, AccountType::Ong) {
        let legal_name = payload.name.trim().to_string();
        let cnpj = payload
            .cnpj
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        sqlx::query(
            r#"
            INSERT INTO ong_profiles (
              id, user_id, legal_name, cnpj, mission, city, state, area_type, contact_phone,
              cep, street, number, complement, neighborhood, foundation_year,
              verification_status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'PENDING_MANUAL_REVIEW')
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(&legal_name)
        .bind(cnpj)
        .bind(default_mission(payload.ong_type.as_deref()))
        .bind(payload.city.as_deref())
        .bind(payload.state.as_deref())
        .bind(payload.ong_type.as_deref())
        .bind(payload.phone.as_deref())
        .bind(payload.cep.as_deref())
        .bind(payload.street.as_deref())
        .bind(payload.number.as_deref())
        .bind(payload.complement.as_deref())
        .bind(payload.neighborhood.as_deref())
        .bind(payload.foundation_year)
        .execute(&mut *tx)
        .await?;

        Some(OngRecord {
            legal_name,
            ong_type: payload.ong_type.clone(),
            cnpj: cnpj.map(str::to_string),
            phone: payload.phone.clone(),
            cep: payload.cep.clone(),
            street: payload.street.clone(),
            number: payload.number.clone(),
            complement: payload.complement.clone(),
            neighborhood: payload.neighborhood.clone(),
            city: payload.city.clone(),
            state: payload.state.clone(),
            foundation_year: payload.foundation_year,
            verification_status: "PENDING_MANUAL_REVIEW".into(),
        })
    } else {
        None
    };

    tx.commit().await?;
    Ok((row_to_user_record(row), ong_record))
}

async fn find_ong_by_user_id(
    state: &AppState,
    user_id: Uuid,
) -> Result<Option<OngRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT legal_name, area_type, cnpj, contact_phone, cep, street, number, complement,
               neighborhood, city, state, foundation_year, verification_status
        FROM ong_profiles
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.map(|row| OngRecord {
        legal_name: row.get("legal_name"),
        ong_type: row.get("area_type"),
        cnpj: row.get("cnpj"),
        phone: row.get("contact_phone"),
        cep: row.get("cep"),
        street: row.get("street"),
        number: row.get("number"),
        complement: row.get("complement"),
        neighborhood: row.get("neighborhood"),
        city: row.get("city"),
        state: row.get("state"),
        foundation_year: row.get("foundation_year"),
        verification_status: row.get("verification_status"),
    }))
}

fn row_to_user_record(row: sqlx::postgres::PgRow) -> UserRecord {
    UserRecord {
        id: row.get("id"),
        name: row.get("name"),
        email: row.get("email"),
        avatar: row.get("avatar_url"),
        password_hash: row.get("password_hash"),
        account_type: auth_service::account_type_from_str(row.get::<&str, _>("account_type")),
        verified: row.get("verified"),
        gender: row.get("gender"),
        cep: row.get("cep"),
        street: row.get("street"),
        number: row.get("number"),
        complement: row.get("complement"),
        neighborhood: row.get("neighborhood"),
        city: row.get("city"),
        state: row.get("state"),
    }
}

async fn queue_password_reset(state: &AppState, email: &str) {
    let Ok(Some(record)) = find_user_by_email(state, email).await else {
        return;
    };
    let token = new_action_token();
    let expires_at = Utc::now() + Duration::minutes(30);
    let inserted = sqlx::query(
        r#"
        INSERT INTO password_reset_tokens (token, user_id, expires_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(&token)
    .bind(record.id)
    .bind(expires_at)
    .execute(&state.db)
    .await;

    if let Err(error) = inserted {
        tracing::warn!(?error, user_id = %record.id, "password reset token was not persisted");
        return;
    }

    if let Err(error) = state.email.send_password_reset(&record.email, &token).await {
        tracing::warn!(?error, user_id = %record.id, "password reset email send failed");
    }
}

fn new_action_token() -> String {
    format!("{}.{}", Uuid::now_v7(), Uuid::now_v7())
}

pub(crate) fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<auth_service::AccessClaims, ApiError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or(ApiError::Unauthorized)?;
    auth_service::verify_access_token(&state.config, token).map_err(|_| ApiError::Unauthorized)
}

pub(crate) async fn audit_event(
    state: &AppState,
    actor_id: Option<Uuid>,
    action: &str,
    metadata: serde_json::Value,
) {
    if let Err(error) = sqlx::query(
        r#"
        INSERT INTO audit_events (actor_user_id, action, metadata)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(actor_id)
    .bind(action)
    .bind(metadata)
    .execute(&state.db)
    .await
    {
        tracing::warn!(?error, ?actor_id, action, "audit event was not persisted");
    }
}

async fn issue_auth_response(
    state: &AppState,
    record: UserRecord,
    ong_record: Option<OngRecord>,
) -> Result<AuthResponse, ApiError> {
    let refresh_token = auth_service::new_refresh_token();
    let expires_at = Utc::now() + Duration::days(state.config.refresh_token_ttl_days);
    let _ = sqlx::query(
        r#"
        INSERT INTO refresh_tokens (token, user_id, expires_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(&refresh_token)
    .bind(record.id)
    .bind(expires_at)
    .execute(&state.db)
    .await;

    let access_token = auth_service::issue_access_token(
        &state.config,
        &record.id.to_string(),
        &record.email,
        record.account_type.clone(),
    )
    .map_err(|error| {
        tracing::error!(?error, "jwt issue failed");
        ApiError::Internal
    })?;

    let profile_address = profile_address_from_record(&record);
    let stats = user_stats(state, record.id).await.unwrap_or_default();
    Ok(auth_response(
        &record.id.to_string(),
        &record.name,
        &record.email,
        record.avatar.as_deref(),
        record.account_type,
        record.verified,
        record.gender,
        profile_address,
        ong_record,
        stats,
        access_token,
        refresh_token,
    ))
}

fn issue_fallback_response(
    state: &AppState,
    id: &str,
    name: &str,
    email: &str,
    avatar: Option<&str>,
    account_type: AccountType,
    gender: Option<String>,
    ong_record: Option<OngRecord>,
) -> Result<AuthResponse, ApiError> {
    let access_token =
        auth_service::issue_access_token(&state.config, id, email, account_type.clone()).map_err(
            |error| {
                tracing::error!(?error, "fallback jwt issue failed");
                ApiError::Internal
            },
        )?;
    Ok(auth_response(
        id,
        name,
        email,
        avatar,
        account_type,
        false,
        gender,
        None,
        ong_record,
        UserStats::default(),
        access_token,
        auth_service::new_refresh_token(),
    ))
}

fn auth_response(
    id: &str,
    name: &str,
    email: &str,
    avatar: Option<&str>,
    account_type: AccountType,
    verified: bool,
    gender: Option<String>,
    profile_address: Option<ProfileAddress>,
    ong_record: Option<OngRecord>,
    stats: UserStats,
    access_token: String,
    refresh_token: String,
) -> AuthResponse {
    let user_verified = if matches!(&account_type, AccountType::Ong) {
        ong_record
            .as_ref()
            .map(|record| record.verification_status == "APPROVED")
            .unwrap_or(false)
    } else {
        verified
    };

    AuthResponse {
        user: UserProfile {
            id: id.into(),
            name: name.into(),
            email: email.into(),
            avatar: avatar.map(str::to_string),
            bio: "Apaixonada por animais".into(),
            account_type,
            verified: user_verified,
            gender,
            posts_count: stats.posts_count,
            helped_count: stats.helped_count,
            adoptions_count: stats.adoptions_count,
            profile_address,
        },
        ong_profile: ong_record.map(|record| OngRegistrationProfile {
            legal_name: record.legal_name,
            ong_type: record.ong_type,
            cnpj: record.cnpj,
            phone: record.phone,
            cep: record.cep,
            street: record.street,
            number: record.number,
            complement: record.complement,
            neighborhood: record.neighborhood,
            city: record.city,
            state: record.state,
            foundation_year: record.foundation_year,
            verification_status: record.verification_status,
        }),
        access_token,
        refresh_token,
        token_type: "Bearer",
    }
}

async fn current_user_response(
    state: &AppState,
    record: UserRecord,
    ong_record: Option<OngRecord>,
) -> CurrentUserResponse {
    let user_verified = if matches!(&record.account_type, AccountType::Ong) {
        ong_record
            .as_ref()
            .map(|record| record.verification_status == "APPROVED")
            .unwrap_or(false)
    } else {
        record.verified
    };

    let profile_address = profile_address_from_record(&record);
    let stats = user_stats(state, record.id).await.unwrap_or_default();

    CurrentUserResponse {
        user: UserProfile {
            id: record.id.to_string(),
            name: record.name,
            email: record.email,
            avatar: record.avatar,
            bio: "Apaixonada por animais".into(),
            account_type: record.account_type,
            verified: user_verified,
            gender: record.gender,
            posts_count: stats.posts_count,
            helped_count: stats.helped_count,
            adoptions_count: stats.adoptions_count,
            profile_address,
        },
        ong_profile: ong_record.map(|record| OngRegistrationProfile {
            legal_name: record.legal_name,
            ong_type: record.ong_type,
            cnpj: record.cnpj,
            phone: record.phone,
            cep: record.cep,
            street: record.street,
            number: record.number,
            complement: record.complement,
            neighborhood: record.neighborhood,
            city: record.city,
            state: record.state,
            foundation_year: record.foundation_year,
            verification_status: record.verification_status,
        }),
    }
}

fn profile_address_from_record(record: &UserRecord) -> Option<ProfileAddress> {
    if [
        record.cep.as_ref(),
        record.street.as_ref(),
        record.number.as_ref(),
        record.complement.as_ref(),
        record.neighborhood.as_ref(),
        record.city.as_ref(),
        record.state.as_ref(),
    ]
    .iter()
    .all(|value| value.is_none())
    {
        return None;
    }

    Some(ProfileAddress {
        cep: record.cep.clone(),
        street: record.street.clone(),
        number: record.number.clone(),
        complement: record.complement.clone(),
        neighborhood: record.neighborhood.clone(),
        city: record.city.clone(),
        state: record.state.clone(),
    })
}

async fn user_stats(state: &AppState, user_id: Uuid) -> Result<UserStats, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT
          COALESCE(posts.count, 0)::int AS posts_count,
          COALESCE(helped.count, 0)::int AS helped_count,
          COALESCE(adoptions.count, 0)::int AS adoptions_count
        FROM users u
        LEFT JOIN LATERAL (
          SELECT count(*) FROM posts p
          WHERE p.author_id = u.id AND p.moderation_status = 'approved'
        ) posts ON true
        LEFT JOIN LATERAL (
          SELECT count(*) FROM rescue_responses rr
          WHERE rr.user_id = u.id AND rr.status IN ('confirmed', 'arrived')
        ) helped ON true
        LEFT JOIN LATERAL (
          SELECT count(*) FROM posts p
          WHERE p.author_id = u.id AND p.post_type = 'adoption'
            AND p.moderation_status = 'approved'
        ) adoptions ON true
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(UserStats {
        posts_count: row.get::<i32, _>("posts_count").max(0) as u32,
        helped_count: row.get::<i32, _>("helped_count").max(0) as u32,
        adoptions_count: row.get::<i32, _>("adoptions_count").max(0) as u32,
    })
}

async fn validate_owned_avatar_url(
    state: &AppState,
    user_id: Uuid,
    avatar_url: &str,
) -> Result<(), ApiError> {
    let expected_prefix = format!(
        "https://res.cloudinary.com/{}/image/upload/",
        state.config.cloudinary_cloud_name
    );
    if !avatar_url.starts_with(&expected_prefix) {
        return Err(ApiError::Validation(
            "avatarUrl must be a Helpin Cloudinary image".into(),
        ));
    }
    if !(avatar_url.contains("/zoohelp/profile-avatars/image/")
        || avatar_url.contains("/zoohelp/ong-logos/image/"))
    {
        return Err(ApiError::Validation(
            "avatarUrl purpose is not allowed".into(),
        ));
    }

    let owned: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
          SELECT 1 FROM media_upload_intents
          WHERE user_id = $1
            AND resource_type = 'image'
            AND expires_at > now() - interval '1 day'
            AND (
              public_url = $2
              OR $2 LIKE '%' || object_key || '%'
            )
        )
        "#,
    )
    .bind(user_id)
    .bind(avatar_url)
    .fetch_one(&state.db)
    .await?;
    if !owned {
        return Err(ApiError::Validation(
            "avatarUrl must come from an upload intent owned by this user".into(),
        ));
    }
    Ok(())
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn validate_ong_payload(
    payload: &RegisterRequest,
    account_type: &AccountType,
) -> Result<(), ApiError> {
    if let Some(gender) = payload
        .gender
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !matches!(gender, "male" | "female") {
            return Err(ApiError::Validation("gender must be male or female".into()));
        }
    }

    if !matches!(account_type, AccountType::Ong) {
        return Ok(());
    }

    if payload.ong_type.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "ongType is required for ONG accounts".into(),
        ));
    }
    if payload.phone.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "phone is required for ONG accounts".into(),
        ));
    }
    if payload.city.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "city is required for ONG accounts".into(),
        ));
    }
    if payload.cep.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "cep is required for ONG accounts".into(),
        ));
    }
    if payload.street.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "street is required for ONG accounts".into(),
        ));
    }
    if payload.number.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "number is required for ONG accounts".into(),
        ));
    }
    if payload.state.as_deref().unwrap_or("").trim().len() != 2 {
        return Err(ApiError::Validation("state must be a 2-letter UF".into()));
    }
    if let Some(year) = payload.foundation_year {
        let current_year = Utc::now().date_naive().format("%Y").to_string();
        let current_year = current_year.parse::<i32>().unwrap_or(2100);
        if year < 1900 || year > current_year {
            return Err(ApiError::Validation("foundationYear is invalid".into()));
        }
    }
    if let Some(cnpj) = payload
        .cnpj
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let digits = cnpj.chars().filter(|ch| ch.is_ascii_digit()).count();
        if digits != 14 {
            return Err(ApiError::Validation(
                "cnpj must contain 14 digits when provided".into(),
            ));
        }
    }

    Ok(())
}

fn normalize_gender(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| matches!(*value, "male" | "female"))
        .map(str::to_string)
}

fn default_mission(ong_type: Option<&str>) -> &'static str {
    match ong_type {
        Some("rescue") => "Resgate e atendimento de animais em situação de risco.",
        Some("adoption") => "Adoção responsável e acompanhamento pós-adoção.",
        Some("vet") | Some("hospital") => "Atendimento veterinário e suporte clínico.",
        Some("welfare") => "Bem-estar animal e proteção comunitária.",
        _ => "Proteção animal e apoio à comunidade.",
    }
}

fn fallback_ong_record(payload: &RegisterRequest, account_type: &AccountType) -> Option<OngRecord> {
    let cnpj = payload
        .cnpj
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    matches!(account_type, AccountType::Ong).then(|| OngRecord {
        legal_name: payload.name.clone(),
        ong_type: payload.ong_type.clone(),
        cnpj,
        phone: payload.phone.clone(),
        cep: payload.cep.clone(),
        street: payload.street.clone(),
        number: payload.number.clone(),
        complement: payload.complement.clone(),
        neighborhood: payload.neighborhood.clone(),
        city: payload.city.clone(),
        state: payload.state.clone(),
        foundation_year: payload.foundation_year,
        verification_status: "PENDING_MANUAL_REVIEW".into(),
    })
}
