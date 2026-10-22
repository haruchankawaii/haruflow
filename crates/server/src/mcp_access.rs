//! Explicit grants and revocable personal credentials for read-only AI tools.
//! Browser sessions manage credentials; bearer credentials authenticate AI reads only.
use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, State},
    http::{HeaderMap, HeaderValue, header, request::Parts},
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row};
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business,
    error::{AppError, AppResult},
};

const SCOPES: [&str; 3] = ["finance", "sales", "inventory"];
const MAX_ACTIVE_TOKENS: i64 = 5;

/// These are AI data capabilities, bounded independently from legacy read routes.
pub fn allowed_scopes(role: &str) -> Vec<String> {
    let scopes: &[&str] = match role {
        "admin" | "viewer" => &SCOPES,
        "finance" => &["finance", "sales"],
        "sales" => &["sales"],
        "warehouse" => &["inventory"],
        _ => &[],
    };
    scopes.iter().map(|scope| (*scope).to_owned()).collect()
}

pub fn effective_scopes(role: &str, grants: &[String], token: &[String]) -> Vec<String> {
    allowed_scopes(role)
        .into_iter()
        .filter(|scope| grants.contains(scope) && token.contains(scope))
        .collect()
}

fn validated_scopes(scopes: Vec<String>) -> AppResult<Vec<String>> {
    if scopes.len() > SCOPES.len() || scopes.iter().any(|s| !SCOPES.contains(&s.as_str())) {
        return Err(AppError::bad_request("Lingkup AI tidak valid."));
    }
    Ok(SCOPES
        .iter()
        .filter(|scope| scopes.iter().any(|value| value == **scope))
        .map(|scope| (*scope).to_owned())
        .collect())
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn bearer(headers: &HeaderMap) -> AppResult<String> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let raw = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or_else(AppError::unauthorized)?;
    if values.next().is_some() {
        return Err(AppError::unauthorized());
    }
    let (scheme, token) = raw.split_once(' ').ok_or_else(AppError::unauthorized)?;
    let valid = scheme.eq_ignore_ascii_case("bearer")
        && token.strip_prefix("hf_mcp_").is_some_and(|secret| {
            secret.len() == 64 && secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    if !valid {
        return Err(AppError::unauthorized());
    }
    Ok(token.to_owned())
}

#[derive(Clone, Debug)]
pub struct McpPrincipal {
    pub user: AuthUser,
    pub token_id: Uuid,
    pub scopes: Vec<String>,
}

impl McpPrincipal {
    pub fn require_scope(&self, scope: &str) -> AppResult<()> {
        if self.scopes.iter().any(|allowed| allowed == scope) {
            Ok(())
        } else {
            Err(AppError::forbidden())
        }
    }
}

impl FromRequestParts<AppState> for McpPrincipal {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        // A session cookie, query parameter, or model-supplied user id is never an AI credential.
        let token = bearer(&parts.headers)?;
        let row = sqlx::query(
            "SELECT u.id,u.name,u.email,u.role,t.id AS token_id,t.scopes AS token_scopes,g.scopes AS grant_scopes \
             FROM ai_tokens t JOIN users u ON u.id=t.user_id \
             JOIN ai_access_grants g ON g.user_id=u.id \
             WHERE t.token_hash=$1 AND t.revoked_at IS NULL AND t.expires_at>now() \
             AND u.active AND g.enabled",
        )
        .bind(hash_token(&token))
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(AppError::unauthorized)?;
        let user = AuthUser {
            id: row.get("id"),
            name: row.get("name"),
            email: row.get("email"),
            role: row.get("role"),
        };
        let scopes = effective_scopes(
            &user.role,
            &row.get::<Vec<String>, _>("grant_scopes"),
            &row.get::<Vec<String>, _>("token_scopes"),
        );
        if scopes.is_empty() {
            return Err(AppError::unauthorized());
        }
        let token_id: Uuid = row.get("token_id");
        sqlx::query("UPDATE ai_tokens SET last_used_at=now() WHERE id=$1")
            .bind(token_id)
            .execute(&state.db)
            .await?;
        Ok(Self {
            user,
            token_id,
            scopes,
        })
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ai/access", get(access))
        .route("/ai/access/{user_id}", put(update_access))
        .route("/ai/tokens", post(create_token))
        .route("/ai/tokens/{id}", delete(revoke_token))
}

#[derive(FromRow, Serialize)]
struct TokenMetadata {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn access(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let row = sqlx::query("SELECT COALESCE(g.enabled,false) AS enabled,COALESCE(g.scopes,'{}'::text[]) AS scopes,u.role FROM users u LEFT JOIN ai_access_grants g ON g.user_id=u.id WHERE u.id=$1 AND u.active")
        .bind(user.id).fetch_one(&state.db).await?;
    let enabled: bool = row.get("enabled");
    let role: String = row.get("role");
    let scopes = if enabled {
        effective_scopes(
            &role,
            &row.get::<Vec<String>, _>("scopes"),
            &allowed_scopes(&role),
        )
    } else {
        vec![]
    };
    let tokens = sqlx::query_as::<_, TokenMetadata>("SELECT id,name,scopes,created_at,expires_at,last_used_at,revoked_at FROM ai_tokens WHERE user_id=$1 ORDER BY created_at DESC,id LIMIT 200")
        .bind(user.id).fetch_all(&state.db).await?;
    let mut response = json!({
        "access":{"user_id":user.id,"enabled":enabled,"scopes":scopes,"allowed_scopes":allowed_scopes(&role)},
        "tokens":tokens,
        "scope_catalog":[{"id":"finance","label":"Keuangan, kas, piutang, dan utang"},{"id":"sales","label":"Penjualan dan pelanggan"},{"id":"inventory","label":"Persediaan dan gudang"}]
    });
    if role == "admin" {
        let rows = sqlx::query("SELECT u.id,u.name,u.email,u.role,u.active,COALESCE(g.enabled,false) AS enabled,COALESCE(g.scopes,'{}'::text[]) AS scopes FROM users u LEFT JOIN ai_access_grants g ON g.user_id=u.id ORDER BY u.name,u.id")
            .fetch_all(&state.db).await?;
        response["users"] = Value::Array(rows.into_iter().map(|row| {
            let role:String=row.get("role");
            let enabled:bool=row.get("enabled");
            let scopes=if enabled { effective_scopes(&role,&row.get::<Vec<String>,_>("scopes"),&allowed_scopes(&role)) } else {vec![]};
            json!({"id":row.get::<Uuid,_>("id"),"name":row.get::<String,_>("name"),"email":row.get::<String,_>("email"),"role":role,"active":row.get::<bool,_>("active"),"enabled":enabled,"scopes":scopes,"allowed_scopes":allowed_scopes(&role)})
        }).collect());
    }
    Ok(Json(response))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantInput {
    enabled: bool,
    scopes: Vec<String>,
}

async fn update_access(
    State(state): State<AppState>,
    user: AuthUser,
    Path(user_id): Path<Uuid>,
    Json(input): Json<GrantInput>,
) -> AppResult<Json<Value>> {
    user.require(&[])?;
    let scopes = validated_scopes(input.scopes)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(492310718002)")
        .execute(&mut *tx)
        .await?;
    crate::auth::ensure_admin(&mut tx, user.id).await?;
    let target = sqlx::query("SELECT role,active FROM users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    let role: String = target.get("role");
    if (input.enabled && (!target.get::<bool, _>("active") || scopes.is_empty()))
        || scopes
            .iter()
            .any(|scope| !allowed_scopes(&role).contains(scope))
    {
        return Err(AppError::bad_request(
            "Pilih lingkup yang sesuai peran untuk akun aktif.",
        ));
    }
    sqlx::query("INSERT INTO ai_access_grants(user_id,enabled,scopes,updated_by) VALUES($1,$2,$3,$4) ON CONFLICT(user_id) DO UPDATE SET enabled=EXCLUDED.enabled,scopes=EXCLUDED.scopes,updated_by=EXCLUDED.updated_by,updated_at=now()")
        .bind(user_id).bind(input.enabled).bind(&scopes).bind(user.id).execute(&mut *tx).await?;
    // Disabling consent permanently revokes old credentials; re-enabling requires a fresh token.
    if !input.enabled {
        sqlx::query("UPDATE ai_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1 AND revoked_at IS NULL")
            .bind(user_id).execute(&mut *tx).await?;
    }
    business::audit(
        &mut tx,
        &user,
        "ai.access.updated",
        "user",
        user_id,
        json!({"enabled":input.enabled,"scopes":scopes}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"user_id":user_id,"enabled":input.enabled,"scopes":scopes,"allowed_scopes":allowed_scopes(&role)}),
    ))
}

fn default_expiry() -> u16 {
    30
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenInput {
    name: String,
    scopes: Vec<String>,
    #[serde(default = "default_expiry")]
    expires_in_days: u16,
}

async fn create_token(
    State(state): State<AppState>,
    user: AuthUser,
    Json(input): Json<TokenInput>,
) -> AppResult<(HeaderMap, Json<Value>)> {
    let name = business::text(&input.name, 80, "Nama koneksi")?;
    let scopes = validated_scopes(input.scopes)?;
    if scopes.is_empty() || !(1..=90).contains(&input.expires_in_days) {
        return Err(AppError::bad_request(
            "Pilih lingkup AI dan masa berlaku 1–90 hari.",
        ));
    }
    let mut tx = state.db.begin().await?;
    // Serialize issuance with grant changes, role changes, and the per-user credential limit.
    let role: String =
        sqlx::query_scalar("SELECT role FROM users WHERE id=$1 AND active FOR UPDATE")
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::unauthorized)?;
    let grants: Option<Vec<String>> =
        sqlx::query_scalar("SELECT scopes FROM ai_access_grants WHERE user_id=$1 AND enabled")
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
    let effective = effective_scopes(&role, &grants.ok_or_else(AppError::forbidden)?, &scopes);
    if effective != scopes {
        return Err(AppError::forbidden());
    }
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM ai_tokens WHERE user_id=$1 AND revoked_at IS NULL AND expires_at>now()")
        .bind(user.id).fetch_one(&mut *tx).await?;
    if count >= MAX_ACTIVE_TOKENS {
        return Err(AppError::conflict(
            "Maksimal lima koneksi AI aktif. Cabut koneksi lama terlebih dahulu.",
        ));
    }
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let token = format!("hf_mcp_{}", hex::encode(secret));
    let id = Uuid::new_v4();
    let metadata=sqlx::query_as::<_,TokenMetadata>("INSERT INTO ai_tokens(id,user_id,name,token_hash,scopes,expires_at) VALUES($1,$2,$3,$4,$5,now()+make_interval(days => $6)) RETURNING id,name,scopes,created_at,expires_at,last_used_at,revoked_at")
        .bind(id).bind(user.id).bind(name).bind(hash_token(&token)).bind(&scopes).bind(i32::from(input.expires_in_days)).fetch_one(&mut *tx).await?;
    business::audit(
        &mut tx,
        &user,
        "ai.token.created",
        "ai_token",
        id,
        json!({"name":metadata.name,"scopes":scopes,"expires_at":metadata.expires_at}),
    )
    .await?;
    tx.commit().await?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    Ok((headers, Json(json!({"token":token,"connection":metadata}))))
}

async fn revoke_token(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    if user.role == "admin" {
        sqlx::query("SELECT pg_advisory_xact_lock(492310718002)")
            .execute(&mut *tx)
            .await?;
        crate::auth::ensure_admin(&mut tx, user.id).await?;
    }
    let owner: Uuid = sqlx::query_scalar("SELECT user_id FROM ai_tokens WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if owner != user.id && user.role != "admin" {
        return Err(AppError::forbidden());
    }
    sqlx::query("UPDATE ai_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    business::audit(
        &mut tx,
        &user,
        "ai.token.revoked",
        "ai_token",
        id,
        json!({"user_id":owner}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"revoked":true})))
}

/// Audit tool identity and result only; never persist credentials or report payloads.
pub async fn audit_call(
    state: &AppState,
    principal: &McpPrincipal,
    tool: &str,
    success: bool,
) -> AppResult<()> {
    let tool = business::text(tool, 80, "Nama tool")?;
    let mut tx = state.db.begin().await?;
    business::audit(
        &mut tx,
        &principal.user,
        "ai.tool.read",
        "ai_token",
        principal.token_id,
        json!({"tool":tool,"success":success}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intersection_never_inherits_admin_bypass_or_role_escalation() {
        let all: Vec<String> = SCOPES.iter().map(|value| (*value).to_owned()).collect();
        assert!(effective_scopes("admin", &[], &all).is_empty());
        assert_eq!(effective_scopes("warehouse", &all, &all), vec!["inventory"]);
        assert_eq!(
            effective_scopes("finance", &all, &all),
            vec!["finance", "sales"]
        );
        assert!(effective_scopes("unknown", &all, &all).is_empty());
    }
    #[test]
    fn bearer_requires_full_secret_and_ignores_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("hf_session=abc"));
        assert!(bearer(&headers).is_err());
        for bad in [
            "Bearer short".to_owned(),
            format!("Basic hf_mcp_{}", "a".repeat(64)),
            format!("Bearer hf_mcp_{}", "g".repeat(64)),
            format!("Bearer hf_mcp_{} trailing", "a".repeat(64)),
        ] {
            headers.insert(header::AUTHORIZATION, HeaderValue::from_str(&bad).unwrap());
            assert!(bearer(&headers).is_err());
        }
        let good = format!("hf_mcp_{}", "a".repeat(64));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("bearer {good}")).unwrap(),
        );
        assert_eq!(bearer(&headers).unwrap(), good);
        headers.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer duplicate"),
        );
        assert!(bearer(&headers).is_err());
    }
}
