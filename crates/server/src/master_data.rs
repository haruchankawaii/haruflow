use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch, post},
};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business::audit,
    domain,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/customers/{id}",
            patch(update_customer).delete(delete_customer),
        )
        .route(
            "/products/{id}",
            patch(update_product).delete(delete_product),
        )
        .route("/accounting/accounts", post(create_account))
        .route("/accounting/accounts/{code}/ledger", get(account_ledger))
}

fn text(value: &str, max: usize, label: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "{label} must contain 1–{max} characters without control characters"
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<String>, max: usize, label: &str) -> AppResult<Option<String>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| text(&value, max, label))
        .transpose()
}

fn address_text(value: Option<String>) -> AppResult<Option<String>> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let value = value.trim();
    if value.chars().count() > 1000
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(AppError::bad_request(
            "Address must contain at most 1000 characters",
        ));
    }
    Ok(Some(value.to_owned()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomerInput {
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
}

async fn update_customer(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CustomerInput>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let name = text(&body.name, 200, "Name")?;
    let email = optional_text(body.email, 254, "Email")?;
    let phone = optional_text(body.phone, 64, "Phone")?;
    let address = address_text(body.address)?;
    let mut tx = state.db.begin().await?;
    let before: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM customers c WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    let result: Value = sqlx::query_scalar(
        "UPDATE customers SET name=$2,email=$3,phone=$4,address=$5 WHERE id=$1 RETURNING to_jsonb(customers)",
    )
    .bind(id).bind(name).bind(email).bind(phone).bind(address)
    .fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "customer.updated",
        "customer",
        id,
        json!({"before":before,"after":result}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn delete_customer(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let mut tx = state.db.begin().await?;
    let customer: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM customers c WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    audit(
        &mut tx,
        &user,
        "customer.deleted",
        "customer",
        id,
        json!({"name":customer["name"]}),
    )
    .await?;
    sqlx::query("DELETE FROM customers WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            deletion_error(
                error,
                "Customer is referenced by a sales document and cannot be deleted",
            )
        })?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductInput {
    sku: String,
    name: String,
    kind: Option<String>,
    unit: Option<String>,
    price: String,
}

const PRODUCT_RETURNING: &str = "UPDATE products SET sku=$2,name=$3,unit=$4,price=$5,kind=$6 WHERE id=$1 RETURNING \
    jsonb_build_object('id',id,'sku',sku,'name',name,'kind',kind,'unit',unit,'price',price::text, \
    'quantity',quantity::text,'average_cost',average_cost::text,'inventory_value',inventory_value::text,'created_at',created_at)";

async fn update_product(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let sku = text(&body.sku.to_uppercase(), 64, "SKU")?;
    let name = text(&body.name, 200, "Name")?;
    let unit = text(body.unit.as_deref().unwrap_or("pcs"), 32, "Unit")?;
    let price = domain::decimal(&body.price, 2, "Price").map_err(AppError::bad_request)?;
    let mut tx = state.db.begin().await?;
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('sku',sku,'name',name,'kind',kind,'unit',unit,'price',price::text) FROM products WHERE id=$1 FOR UPDATE",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    let kind = crate::business::product_kind(
        body.kind
            .as_deref()
            .unwrap_or_else(|| before["kind"].as_str().unwrap()),
    )?;
    if before["kind"] != kind {
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sales_order_lines WHERE product_id=$1) OR EXISTS(SELECT 1 FROM inventory_movements WHERE product_id=$1)",
        ).bind(id).fetch_one(&mut *tx).await?;
        if used {
            return Err(AppError::conflict(
                "Product kind cannot change after use in a sales or stock document",
            ));
        }
    }
    // Inventory balances are deliberately absent from both input and UPDATE columns.
    // Existing sales lines retain their product_sku/product_name/unit_price snapshots.
    let result: Value = sqlx::query_scalar(PRODUCT_RETURNING)
        .bind(id)
        .bind(sku)
        .bind(name)
        .bind(unit)
        .bind(price)
        .bind(kind)
        .fetch_one(&mut *tx)
        .await?;
    let after = json!({"sku":result["sku"],"name":result["name"],"kind":result["kind"],"unit":result["unit"],"price":result["price"]});
    audit(
        &mut tx,
        &user,
        "product.updated",
        "product",
        id,
        json!({"before":before,"after":after}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn delete_product(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let mut tx = state.db.begin().await?;
    let product = sqlx::query(
        "SELECT sku,name,quantity,inventory_value FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(AppError::not_found)?;
    if product.get::<Decimal, _>("quantity") != Decimal::ZERO
        || product.get::<Decimal, _>("inventory_value") != Decimal::ZERO
    {
        return Err(AppError::conflict("Product with stock cannot be deleted"));
    }
    audit(
        &mut tx,
        &user,
        "product.deleted",
        "product",
        id,
        json!({
            "sku":product.get::<String,_>("sku"),"name":product.get::<String,_>("name")
        }),
    )
    .await?;
    sqlx::query("DELETE FROM products WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            deletion_error(
                error,
                "Product is referenced by a sales or stock document and cannot be deleted",
            )
        })?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

fn deletion_error(error: sqlx::Error, message: &str) -> AppError {
    if error
        .as_database_error()
        .is_some_and(|error| error.is_foreign_key_violation())
    {
        AppError::conflict(message)
    } else {
        error.into()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountInput {
    code: String,
    name: String,
    kind: String,
}

fn account_code(value: &str) -> AppResult<String> {
    if (3..=10).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_digit()) {
        Ok(value.to_owned())
    } else {
        Err(AppError::bad_request(
            "Account code must contain 3–10 digits",
        ))
    }
}

fn account_kind(value: &str) -> AppResult<&str> {
    if ["asset", "liability", "equity", "revenue", "expense"].contains(&value) {
        Ok(value)
    } else {
        Err(AppError::bad_request(
            "Account kind must be asset, liability, equity, revenue, or expense",
        ))
    }
}

async fn create_account(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AccountInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let code = account_code(&body.code)?;
    let name = text(&body.name, 200, "Account name")?;
    let kind = account_kind(&body.kind)?;
    let mut tx = state.db.begin().await?;
    let result: Value = sqlx::query_scalar(
        "INSERT INTO accounts(code,name,kind) VALUES($1,$2,$3) RETURNING to_jsonb(accounts)",
    )
    .bind(&code)
    .bind(name)
    .bind(kind)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO audit_events(id,actor_id,action,resource_type,details) VALUES($1,$2,'account.created','account',$3)")
        .bind(Uuid::new_v4()).bind(user.id).bind(json!({"account":result})).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerFilter {
    from: Option<String>,
    to: Option<String>,
}

fn date_filter(value: Option<&str>) -> AppResult<Option<NaiveDate>> {
    value
        .map(|value| {
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| AppError::bad_request("Ledger dates must use YYYY-MM-DD"))?;
            if !(1..=9999).contains(&date.year()) || date.format("%Y-%m-%d").to_string() != value {
                return Err(AppError::bad_request("Ledger dates must use YYYY-MM-DD"));
            }
            Ok(date)
        })
        .transpose()
}

async fn account_ledger(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(code): Path<String>,
    Query(filter): Query<LedgerFilter>,
) -> AppResult<Json<Value>> {
    let code = account_code(&code)?;
    let from = date_filter(filter.from.as_deref())?;
    let to = date_filter(filter.to.as_deref())?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err(AppError::bad_request(
            "Ledger from date must be on or before to date",
        ));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let account: Value = sqlx::query_scalar("SELECT to_jsonb(a) FROM accounts a WHERE code=$1")
        .bind(&code)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let timezone: String = sqlx::query_scalar("SELECT timezone FROM company")
        .fetch_one(&mut *tx)
        .await?;
    // Group multiple lines for the same account in one journal into a single ledger row.
    // Aggregation and the closing balance use the entire range; only presentation is capped.
    let mut result: Value = sqlx::query_scalar(r#"
        WITH entries AS (
            SELECT j.id AS journal_id,j.description,j.created_at,j.source_type,
                sum(l.debit) AS debit,sum(l.credit) AS credit,
                (j.created_at AT TIME ZONE $2)::date AS posting_date
            FROM journal_entries j JOIN journal_lines l ON l.journal_id=j.id
            WHERE l.account_code=$1
            GROUP BY j.id,j.description,j.created_at,j.source_type
        ), opening AS (
            SELECT COALESCE(sum(debit-credit) FILTER (
                WHERE $3::date IS NOT NULL AND posting_date<$3),0) AS balance FROM entries
        ), selected AS (
            SELECT * FROM entries WHERE ($3::date IS NULL OR posting_date>=$3)
                AND ($4::date IS NULL OR posting_date<=$4)
        ), running AS (
            SELECT *, (SELECT balance FROM opening) + sum(debit-credit) OVER (
                ORDER BY created_at,journal_id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS balance
            FROM selected
        ), visible AS (
            SELECT * FROM running ORDER BY created_at,journal_id LIMIT 5000
        )
        SELECT jsonb_build_object(
            'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
                'journal_id',journal_id,'description',description,'created_at',created_at,'source_type',source_type,
                'debit',debit::text,'credit',credit::text,'balance',balance::text)
                ORDER BY created_at,journal_id) FROM visible),'[]'::jsonb),
            'opening_balance',(SELECT balance FROM opening)::text,
            'closing_balance',((SELECT balance FROM opening) + COALESCE((SELECT sum(debit-credit) FROM selected),0))::text,
            'truncated',(SELECT count(*) FROM selected)>5000)
    "#).bind(&code).bind(timezone).bind(from).bind(to).fetch_one(&mut *tx).await?;
    result["account"] = account;
    tx.commit().await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_codes_and_kinds_are_explicit() {
        for code in ["1100", "123", "0000000001"] {
            assert!(account_code(code).is_ok());
        }
        for code in ["12", "12345678901", " 1100", "1100 ", "11A0", "１１００"] {
            assert!(account_code(code).is_err());
        }
        for kind in ["asset", "liability", "equity", "revenue", "expense"] {
            assert!(account_kind(kind).is_ok());
        }
        assert!(account_kind("cash").is_err());
    }

    #[test]
    fn ledger_dates_are_strict_and_calendar_validated() {
        assert!(date_filter(None).unwrap().is_none());
        assert_eq!(
            date_filter(Some("2024-02-29"))
                .unwrap()
                .unwrap()
                .to_string(),
            "2024-02-29"
        );
        for date in [
            "2025-02-29",
            "2026-2-01",
            "2026-02-1",
            "2026-02-01T00:00:00Z",
            "",
            "0000-01-01",
        ] {
            assert!(
                date_filter(Some(date)).is_err(),
                "unexpectedly accepted {date}"
            );
        }
    }

    #[test]
    fn product_patch_rejects_inventory_mutation_fields() {
        let body =
            json!({"sku":"TEST","name":"Product","unit":"pcs","price":"1000","quantity":"100"});
        assert!(serde_json::from_value::<ProductInput>(body).is_err());
        assert!(text("hello\u{0}world", 200, "Name").is_err());
    }
}
