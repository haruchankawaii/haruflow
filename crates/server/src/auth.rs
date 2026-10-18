use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, State},
    http::{HeaderMap, HeaderValue, header, request::Parts},
    routing::{get, patch, post},
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

const SESSION_SECONDS: u64 = 7 * 24 * 60 * 60;
const ATTEMPT_WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_LOGIN_FAILURES: u32 = 8;

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
}

impl AuthUser {
    pub fn require(&self, roles: &[&str]) -> AppResult<()> {
        if self.role == "admin" || roles.contains(&self.role.as_str()) {
            Ok(())
        } else {
            Err(AppError::forbidden())
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let token = session_token(&parts.headers).ok_or_else(AppError::unauthorized)?;
        sqlx::query_as::<_, AuthUser>(
            "SELECT u.id, u.name, u.email, u.role FROM sessions s \
             JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = $1 AND s.expires_at > now() AND u.active",
        )
        .bind(token_hash(&token))
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(AppError::unauthorized)
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(setup_status))
        .route("/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/audit-events", get(list_audit_events))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", patch(update_user))
        .route("/users/{id}/password", post(reset_password))
}

async fn setup_status(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM company)")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "needs_setup": !exists })))
}

#[derive(Deserialize)]
struct SetupInput {
    token: String,
    company_name: String,
    name: String,
    email: String,
    password: String,
}

async fn setup(
    State(state): State<AppState>,
    Json(input): Json<SetupInput>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    if state.setup_token.is_empty() || !tokens_match(&input.token, &state.setup_token) {
        return Err(AppError::forbidden());
    }
    let company_name = validated_text(
        &input.company_name,
        2,
        120,
        "Nama perusahaan harus 2–120 karakter.",
    )?;
    let name = validated_text(&input.name, 2, 100, "Nama harus 2–100 karakter.")?;
    let email = normalize_email(&input.email)?;
    let password_hash = hash_password(input.password).await?;
    let mut tx = state.db.begin().await?;
    // Every setup request takes the same lock before checking the singleton company.
    sqlx::query("SELECT pg_advisory_xact_lock(492310718001)")
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM company)")
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        return Err(AppError::conflict("Perusahaan sudah disiapkan."));
    }
    let company_id = Uuid::new_v4();
    let user = AuthUser {
        id: Uuid::new_v4(),
        name,
        email,
        role: "admin".to_owned(),
    };
    sqlx::query("INSERT INTO company(id, name) VALUES ($1, $2)")
        .bind(company_id)
        .bind(&company_name)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO users(id, name, email, password_hash, role) VALUES ($1, $2, $3, $4, 'admin')",
    )
    .bind(user.id)
    .bind(&user.name)
    .bind(&user.email)
    .bind(password_hash)
    .execute(&mut *tx)
    .await?;
    write_audit(
        &mut tx,
        user.id,
        "company.setup",
        "company",
        company_id,
        json!({ "name": company_name }),
    )
    .await?;
    let token = create_session(&mut tx, user.id).await?;
    tx.commit().await?;
    Ok((
        session_headers(&token, state.secure_cookies)?,
        Json(json!({
            "user": user,
            "company": {"id": company_id, "name": company_name, "currency": "IDR", "timezone": "Asia/Jakarta"}
        })),
    ))
}

#[derive(Deserialize)]
struct LoginInput {
    email: String,
    password: String,
}

#[derive(FromRow)]
struct LoginUser {
    id: Uuid,
    password_hash: String,
    active: bool,
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    let email = input.email.trim().to_ascii_lowercase();
    if email.len() > 254 || input.password.len() > 128 || input.password.is_empty() {
        return Err(AppError::unauthorized());
    }
    reserve_login_attempt(&email)?;
    let user = sqlx::query_as::<_, LoginUser>(
        "SELECT id, password_hash, active FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;
    // Missing accounts also perform Argon2 verification to avoid a cheap account probe.
    let valid = verify_password(
        input.password,
        user.as_ref().map(|u| u.password_hash.clone()),
    )
    .await?;
    let user = match user {
        Some(user) if valid && user.active => user,
        _ => {
            return Err(AppError::unauthorized());
        }
    };
    clear_login_failures(&email);
    let mut tx = state.db.begin().await?;
    // Recheck under the same row lock used by account deactivation/password reset.
    let user = sqlx::query_as::<_, AuthUser>(
        "SELECT id, name, email, role FROM users \
         WHERE id = $1 AND active AND password_hash = $2 FOR UPDATE",
    )
    .bind(user.id)
    .bind(&user.password_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(AppError::unauthorized)?;
    sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    let token = create_session(&mut tx, user.id).await?;
    write_audit(&mut tx, user.id, "auth.login", "user", user.id, json!({})).await?;
    tx.commit().await?;
    Ok((
        session_headers(&token, state.secure_cookies)?,
        Json(context(&state, user).await?),
    ))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    if let Some(token) = session_token(&headers) {
        let mut tx = state.db.begin().await?;
        let actor: Option<Uuid> =
            sqlx::query_scalar("DELETE FROM sessions WHERE token_hash = $1 RETURNING user_id")
                .bind(token_hash(&token))
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(id) = actor {
            write_audit(&mut tx, id, "auth.logout", "user", id, json!({})).await?;
        }
        tx.commit().await?;
    }
    let mut response_headers = HeaderMap::new();
    let secure = if state.secure_cookies { "; Secure" } else { "" };
    response_headers.insert(
        header::SET_COOKIE,
        header_value(&format!(
            "hf_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}"
        ))?,
    );
    Ok((response_headers, Json(json!({"ok": true}))))
}

async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(context(&state, user).await?))
}

#[derive(Deserialize)]
struct ChangePasswordInput {
    current_password: String,
    password: String,
}

async fn change_password(
    State(state): State<AppState>,
    actor: AuthUser,
    Json(input): Json<ChangePasswordInput>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    if input.current_password.is_empty() || input.current_password.len() > 128 {
        return Err(AppError::unauthorized());
    }
    reserve_login_attempt(&actor.email)?;
    let original_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1 AND active")
            .bind(actor.id)
            .fetch_optional(&state.db)
            .await?;
    if !verify_password(input.current_password, original_hash.clone()).await? {
        return Err(AppError::unauthorized());
    }
    let new_hash = hash_password(input.password).await?;
    let mut tx = state.db.begin().await?;
    // A concurrent reset/deactivation must invalidate this request before it changes anything.
    let user = sqlx::query_as::<_, AuthUser>(
        "SELECT id, name, email, role FROM users \
         WHERE id = $1 AND active AND password_hash = $2 FOR UPDATE",
    )
    .bind(actor.id)
    .bind(original_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(AppError::unauthorized)?;
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
        .bind(actor.id)
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    let token = create_session(&mut tx, actor.id).await?;
    write_audit(
        &mut tx,
        actor.id,
        "user.password_change",
        "user",
        actor.id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    clear_login_failures(&actor.email);
    Ok((
        session_headers(&token, state.secure_cookies)?,
        Json(context(&state, user).await?),
    ))
}

#[derive(Serialize, FromRow)]
struct AuditView {
    id: Uuid,
    actor_id: Option<Uuid>,
    actor_name: Option<String>,
    actor_email: Option<String>,
    action: String,
    resource_type: String,
    resource_id: Option<Uuid>,
    details: serde_json::Value,
    created_at: DateTime<Utc>,
}

async fn list_audit_events(
    State(state): State<AppState>,
    actor: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    actor.require(&["finance"])?;
    let events = sqlx::query_as::<_, AuditView>(
        "SELECT e.id, e.actor_id, u.name AS actor_name, u.email AS actor_email, e.action, \
         e.resource_type, e.resource_id, e.details, e.created_at \
         FROM audit_events e LEFT JOIN users u ON e.actor_id = u.id \
         ORDER BY e.created_at DESC, e.id DESC LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({"items": events})))
}

async fn context(state: &AppState, user: AuthUser) -> AppResult<serde_json::Value> {
    let company = crate::company::profile(&state.db).await?;
    Ok(json!({ "user": user, "company": company }))
}

#[derive(Serialize, FromRow)]
struct UserView {
    id: Uuid,
    name: String,
    email: String,
    role: String,
    active: bool,
    created_at: DateTime<Utc>,
}

async fn list_users(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    user.require(&[])?;
    let users = sqlx::query_as::<_, UserView>(
        "SELECT id, name, email, role, active, created_at FROM users ORDER BY created_at, id",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": users })))
}

#[derive(Deserialize)]
struct CreateUserInput {
    name: String,
    email: String,
    password: String,
    role: String,
}

async fn create_user(
    State(state): State<AppState>,
    actor: AuthUser,
    Json(input): Json<CreateUserInput>,
) -> AppResult<Json<serde_json::Value>> {
    actor.require(&[])?;
    let name = validated_text(&input.name, 2, 100, "Nama harus 2–100 karakter.")?;
    let email = normalize_email(&input.email)?;
    validate_role(&input.role)?;
    let password_hash = hash_password(input.password).await?;
    let id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    ensure_admin(&mut tx, actor.id).await?;
    let inserted = sqlx::query_as::<_, UserView>(
        "INSERT INTO users(id, name, email, password_hash, role) VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT(email) DO NOTHING RETURNING id, name, email, role, active, created_at",
    )
    .bind(id)
    .bind(&name)
    .bind(&email)
    .bind(password_hash)
    .bind(&input.role)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::conflict("Email sudah digunakan."))?;
    write_audit(
        &mut tx,
        actor.id,
        "user.create",
        "user",
        id,
        json!({"role": input.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "user": inserted })))
}

#[derive(Deserialize)]
struct UpdateUserInput {
    active: Option<bool>,
    role: Option<String>,
}

async fn update_user(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateUserInput>,
) -> AppResult<Json<serde_json::Value>> {
    actor.require(&[])?;
    if input.active.is_none() && input.role.is_none() {
        return Err(AppError::bad_request(
            "Isi status aktif atau peran pengguna.",
        ));
    }
    if let Some(role) = &input.role {
        validate_role(role)?;
    }
    if id == actor.id
        && (input.active == Some(false) || input.role.as_ref().is_some_and(|r| r != "admin"))
    {
        return Err(AppError::bad_request(
            "Administrator tidak dapat menonaktifkan atau menurunkan perannya sendiri.",
        ));
    }
    let mut tx = state.db.begin().await?;
    // A shared advisory lock serializes administrator changes and prevents cross-demotion races.
    sqlx::query("SELECT pg_advisory_xact_lock(492310718002)")
        .execute(&mut *tx)
        .await?;
    ensure_admin(&mut tx, actor.id).await?;
    let updated = sqlx::query_as::<_, UserView>(
        "UPDATE users SET active = COALESCE($2, active), role = COALESCE($3, role), updated_at = now() \
         WHERE id = $1 RETURNING id, name, email, role, active, created_at",
    )
    .bind(id)
    .bind(input.active)
    .bind(&input.role)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::bad_request("Pengguna tidak ditemukan."))?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    write_audit(
        &mut tx,
        actor.id,
        "user.update",
        "user",
        id,
        json!({"active": input.active, "role": input.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "user": updated })))
}

#[derive(Deserialize)]
struct PasswordInput {
    password: String,
}

async fn reset_password(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<PasswordInput>,
) -> AppResult<Json<serde_json::Value>> {
    actor.require(&[])?;
    let password_hash = hash_password(input.password).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(492310718002)")
        .execute(&mut *tx)
        .await?;
    ensure_admin(&mut tx, actor.id).await?;
    let affected =
        sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(password_hash)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("Pengguna tidak ditemukan."));
    }
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    write_audit(
        &mut tx,
        actor.id,
        "user.password_reset",
        "user",
        id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn ensure_admin(connection: &mut PgConnection, id: Uuid) -> AppResult<()> {
    let allowed: Option<bool> =
        sqlx::query_scalar("SELECT active AND role = 'admin' FROM users WHERE id = $1 FOR SHARE")
            .bind(id)
            .fetch_optional(connection)
            .await?;
    if allowed == Some(true) {
        Ok(())
    } else {
        Err(AppError::forbidden())
    }
}

async fn create_session(connection: &mut PgConnection, user_id: Uuid) -> AppResult<String> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    sqlx::query("INSERT INTO sessions(token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '7 days')")
        .bind(token_hash(&token)).bind(user_id).execute(connection).await?;
    Ok(token)
}

async fn write_audit(
    connection: &mut PgConnection,
    actor_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    details: serde_json::Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO audit_events(id, actor_id, action, resource_type, resource_id, details) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(actor_id).bind(action).bind(resource_type).bind(resource_id).bind(details)
        .execute(connection).await?;
    Ok(())
}

fn session_headers(token: &str, secure: bool) -> AppResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    let secure = if secure { "; Secure" } else { "" };
    headers.insert(header::SET_COOKIE, header_value(&format!(
        "hf_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={SESSION_SECONDS}{secure}"
    ))?);
    Ok(headers)
}

fn header_value(value: &str) -> AppResult<HeaderValue> {
    HeaderValue::from_str(value).map_err(|error| AppError::internal(error.into()))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == "hf_session" && value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()))
            .then(|| value.to_owned())
    })
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn tokens_match(left: &str, right: &str) -> bool {
    let left = Sha256::digest(left.as_bytes());
    let right = Sha256::digest(right.as_bytes());
    left.iter()
        .zip(right.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn validated_text(value: &str, min: usize, max: usize, message: &str) -> AppResult<String> {
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count()) || value.chars().any(char::is_control) {
        Err(AppError::bad_request(message))
    } else {
        Ok(value.to_owned())
    }
}

pub(crate) fn normalize_email(value: &str) -> AppResult<String> {
    let email = value.trim().to_ascii_lowercase();
    let valid = email.is_ascii()
        && email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && local.len() <= 64
                && !local.starts_with('.')
                && !local.ends_with('.')
                && !local.contains("..")
                && local
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
                && domain.contains('.')
                && domain.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
        });
    if valid {
        Ok(email)
    } else {
        Err(AppError::bad_request("Alamat email tidak valid."))
    }
}

fn validate_role(role: &str) -> AppResult<()> {
    if ["admin", "sales", "warehouse", "finance", "viewer"].contains(&role) {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "Peran harus admin, sales, warehouse, finance, atau viewer.",
        ))
    }
}

fn password_permit() -> AppResult<tokio::sync::OwnedSemaphorePermit> {
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::bad_request("Layanan kata sandi sedang sibuk. Coba kembali sebentar lagi.")
        })
}

pub(crate) async fn hash_password(password: String) -> AppResult<String> {
    if !(12..=128).contains(&password.len()) {
        return Err(AppError::bad_request("Kata sandi harus 12–128 byte."));
    }
    let permit = password_permit()?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|hash| hash.to_string())
            .map_err(|error| {
                AppError::internal(anyhow::anyhow!("Password hashing failed: {error}"))
            })
    })
    .await
    .map_err(|error| AppError::internal(error.into()))?
}

async fn verify_password(password: String, hash: Option<String>) -> AppResult<bool> {
    let permit = password_permit()?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        static DUMMY_HASH: OnceLock<String> = OnceLock::new();
        let fallback = DUMMY_HASH.get_or_init(|| {
            Argon2::default()
                .hash_password(
                    b"haruflow-account-probe-defense",
                    &SaltString::generate(&mut OsRng),
                )
                .expect("Argon2 default parameters are valid")
                .to_string()
        });
        let stored = hash.as_deref().unwrap_or(fallback);
        let valid = PasswordHash::new(stored).ok().is_some_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        });
        hash.is_some() && valid
    })
    .await
    .map_err(|error| AppError::internal(error.into()))
}

struct Attempts {
    started: Instant,
    failures: u32,
}
fn attempts() -> &'static Mutex<HashMap<String, Attempts>> {
    static ATTEMPTS: OnceLock<Mutex<HashMap<String, Attempts>>> = OnceLock::new();
    ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn reserve_login_attempt(email: &str) -> AppResult<()> {
    let mut attempts = attempts()
        .lock()
        .map_err(|_| AppError::internal(anyhow::anyhow!("Login throttle lock failed")))?;
    attempts.retain(|_, attempt| attempt.started.elapsed() < ATTEMPT_WINDOW);
    if attempts
        .get(email)
        .is_some_and(|attempt| attempt.failures >= MAX_LOGIN_FAILURES)
    {
        return Err(AppError::bad_request(
            "Terlalu banyak percobaan masuk. Coba kembali setelah 15 menit.",
        ));
    }
    // Bound memory even when clients deliberately submit distinct account names.
    if attempts.len() >= 10_000 && !attempts.contains_key(email) {
        return Err(AppError::bad_request(
            "Layanan masuk sedang sibuk. Coba kembali nanti.",
        ));
    }
    let attempt = attempts.entry(email.to_owned()).or_insert(Attempts {
        started: Instant::now(),
        failures: 0,
    });
    attempt.failures = attempt.failures.saturating_add(1);
    Ok(())
}

fn clear_login_failures(email: &str) {
    if let Ok(mut attempts) = attempts().lock() {
        attempts.remove(email);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization_and_validation() {
        assert_eq!(
            normalize_email("  Owner+ERP@Example.COM ").unwrap(),
            "owner+erp@example.com"
        );
        for email in [
            "bad",
            "a@@example.com",
            "a b@example.com",
            ".a@example.com",
            "a@-example.com",
            "a@example..com",
            "a..b@example.com",
        ] {
            assert!(
                normalize_email(email).is_err(),
                "unexpectedly valid: {email}"
            );
        }
    }

    #[test]
    fn cookie_parser_only_accepts_full_session_tokens() {
        let mut headers = HeaderMap::new();
        let token = "a".repeat(64);
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("other=1; hf_session={token}; theme=dark")).unwrap(),
        );
        assert_eq!(session_token(&headers), Some(token));
        headers.insert(header::COOKIE, HeaderValue::from_static("hf_session=short"));
        assert!(session_token(&headers).is_none());
    }

    #[test]
    fn permissions_and_setup_tokens() {
        let mut user = AuthUser {
            id: Uuid::new_v4(),
            name: "Owner".into(),
            email: "owner@example.com".into(),
            role: "sales".into(),
        };
        assert!(user.require(&["sales"]).is_ok());
        assert!(user.require(&["finance"]).is_err());
        assert!(user.require(&[]).is_err());
        user.role = "admin".into();
        assert!(user.require(&[]).is_ok());
        assert!(tokens_match("secret", "secret"));
        assert!(!tokens_match("secret", "different"));
    }

    #[test]
    fn session_hash_and_supported_roles() {
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        for role in ["admin", "sales", "warehouse", "finance", "viewer"] {
            assert!(validate_role(role).is_ok());
        }
        for role in ["root", "Admin", "manager", "", "sales "] {
            assert!(validate_role(role).is_err());
        }
    }

    #[test]
    fn login_attempts_are_bounded_and_reset_after_success() {
        let email = format!("throttle-test-{}@example.com", Uuid::new_v4());
        for _ in 0..MAX_LOGIN_FAILURES {
            assert!(reserve_login_attempt(&email).is_ok());
        }
        assert!(reserve_login_attempt(&email).is_err());
        clear_login_failures(&email);
        assert!(reserve_login_attempt(&email).is_ok());
        clear_login_failures(&email);
    }

    #[tokio::test]
    async fn password_roundtrip_and_limits() {
        assert!(hash_password("short".into()).await.is_err());
        let password = "a sufficiently long secret".to_owned();
        let hash = hash_password(password.clone()).await.unwrap();
        assert_ne!(hash, password);
        assert!(verify_password(password, Some(hash.clone())).await.unwrap());
        assert!(
            !verify_password("wrong password".into(), Some(hash))
                .await
                .unwrap()
        );
        assert!(
            !verify_password("haruflow-account-probe-defense".into(), None)
                .await
                .unwrap()
        );
    }
}
