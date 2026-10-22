use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{self, AuthUser},
    business::audit,
    error::{AppError, AppResult},
};

#[derive(Serialize, FromRow, PartialEq)]
pub(crate) struct CompanyView {
    pub id: Uuid,
    pub name: String,
    pub currency: String,
    pub timezone: String,
    pub address: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub tax_id: Option<String>,
    pub order_prefix: String,
    pub invoice_prefix: String,
}

const COMPANY_SELECT: &str = "SELECT id, name, currency, timezone, address, email, phone, tax_id, order_prefix, invoice_prefix FROM company";

pub fn router() -> Router<AppState> {
    Router::new().route("/company", get(get_company).patch(update_company))
}

pub(crate) async fn profile(pool: &PgPool) -> AppResult<CompanyView> {
    Ok(sqlx::query_as(COMPANY_SELECT).fetch_one(pool).await?)
}

async fn get_company(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    Ok(Json(json!(profile(&state.db).await?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompanyInput {
    name: String,
    timezone: String,
    address: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    tax_id: Option<String>,
    order_prefix: String,
    invoice_prefix: String,
}

fn validated_text(value: &str, min: usize, max: usize, label: &str) -> AppResult<String> {
    // Reject controls before trimming, so e.g. a pasted newline cannot hide in an audit value.
    if value.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "{label} tidak boleh berisi karakter kontrol."
        )));
    }
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count()) {
        return Err(AppError::bad_request(format!(
            "{label} harus {min}–{max} karakter."
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<String>, max: usize, label: &str) -> AppResult<Option<String>> {
    value
        .map(|value| {
            if value.chars().any(char::is_control) {
                return Err(AppError::bad_request(format!(
                    "{label} tidak boleh berisi karakter kontrol."
                )));
            }
            if value.trim().is_empty() {
                return Ok(None);
            }
            validated_text(&value, 1, max, label).map(Some)
        })
        .transpose()
        .map(Option::flatten)
}

fn prefix(value: &str, label: &str) -> AppResult<String> {
    let value = validated_text(value, 2, 10, label)?;
    if !value.as_bytes()[0].is_ascii_uppercase()
        || !value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(AppError::bad_request(format!(
            "{label} harus diawali huruf kapital dan hanya berisi A–Z, 0–9, atau tanda hubung."
        )));
    }
    Ok(value)
}

fn optional_address(value: Option<String>) -> AppResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    if value
        .chars()
        .any(|character| character.is_control() && character != '\n')
    {
        return Err(AppError::bad_request(
            "Alamat berisi karakter kontrol yang tidak valid.",
        ));
    }
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 500 {
        return Err(AppError::bad_request("Alamat maksimal 500 karakter."));
    }
    Ok(Some(value.to_owned()))
}

async fn update_company(
    State(state): State<AppState>,
    actor: AuthUser,
    Json(input): Json<CompanyInput>,
) -> AppResult<Json<Value>> {
    actor.require(&[])?;
    let name = validated_text(&input.name, 2, 120, "Nama perusahaan")?;
    let timezone = validated_text(&input.timezone, 1, 100, "Zona waktu")?;
    let address = optional_address(input.address)?;
    let email = optional_text(input.email, 254, "Email")?
        .map(|value| auth::normalize_email(&value))
        .transpose()?;
    let phone = optional_text(input.phone, 40, "Telepon")?;
    let tax_id = optional_text(input.tax_id, 64, "Nomor pajak")?;
    let order_prefix = prefix(&input.order_prefix, "Prefix pesanan")?;
    let invoice_prefix = prefix(&input.invoice_prefix, "Prefix invoice")?;

    let mut tx = state.db.begin().await?;
    // The accounting lock is also taken by every posting and period close. Holding it
    // before the company row lock prevents a timezone edit racing the first journal.
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    auth::ensure_admin(&mut tx, actor.id).await?;
    let previous: CompanyView = sqlx::query_as(&format!("{COMPANY_SELECT} FOR UPDATE"))
        .fetch_one(&mut *tx)
        .await?;
    let known_timezone: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)")
            .bind(&timezone)
            .fetch_one(&mut *tx)
            .await?;
    if !known_timezone {
        return Err(AppError::bad_request("Zona waktu IANA tidak valid."));
    }
    if previous.timezone != timezone {
        let has_postings: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM journal_entries)")
            .fetch_one(&mut *tx)
            .await?;
        if has_postings {
            return Err(AppError::conflict(
                "Zona waktu tidak dapat diubah setelah jurnal pertama diposting. Profil dan prefix tetap dapat diperbarui.",
            ));
        }
    }
    let updated: CompanyView = sqlx::query_as(
        "UPDATE company SET name=$1, timezone=$2, address=$3, email=$4, phone=$5, tax_id=$6, order_prefix=$7, invoice_prefix=$8 \
         WHERE id=$9 RETURNING id, name, currency, timezone, address, email, phone, tax_id, order_prefix, invoice_prefix",
    )
    .bind(name)
    .bind(timezone)
    .bind(address)
    .bind(email)
    .bind(phone)
    .bind(tax_id)
    .bind(order_prefix)
    .bind(invoice_prefix)
    .bind(previous.id)
    .fetch_one(&mut *tx)
    .await?;
    if previous != updated {
        // Field names are sufficient for the audit trail; do not copy contact details
        // or the raw request into the immutable event log.
        let before = serde_json::to_value(&previous).map_err(|e| AppError::internal(e.into()))?;
        let after = serde_json::to_value(&updated).map_err(|e| AppError::internal(e.into()))?;
        let changed_fields: Vec<_> = after
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| before.get(*key) != after.get(*key))
            .cloned()
            .collect();
        audit(
            &mut tx,
            &actor,
            "company.profile_updated",
            "company",
            previous.id,
            json!({ "changed_fields": changed_fields }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!(updated)))
}

/// Local operator recovery only: the CLI supplies the password through a hidden
/// prompt. Existing inactive or non-admin users cannot gain access through recovery.
pub async fn reset_admin_password(pool: &PgPool, email: &str, password: String) -> AppResult<()> {
    let email = auth::normalize_email(email)?;
    let password_hash = auth::hash_password(password).await?;
    let mut tx = pool.begin().await?;
    // Share the lock used for role changes and normal administrative password resets.
    sqlx::query("SELECT pg_advisory_xact_lock(492310718002)")
        .execute(&mut *tx)
        .await?;
    let actor: AuthUser = sqlx::query_as(
        "SELECT id, name, email, role FROM users WHERE email=$1 AND active AND role='admin' FOR UPDATE",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::bad_request("Administrator aktif dengan email tersebut tidak ditemukan."))?;
    sqlx::query("UPDATE users SET password_hash=$2, updated_at=now() WHERE id=$1")
        .bind(actor.id)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE ai_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1 AND revoked_at IS NULL")
        .bind(actor.id).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &actor,
        "user.password_reset_local",
        "user",
        actor.id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
