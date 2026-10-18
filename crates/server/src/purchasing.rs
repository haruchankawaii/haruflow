use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    domain,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/suppliers", get(suppliers).post(create_supplier))
        .route("/suppliers/{id}", axum::routing::put(update_supplier))
        .route("/purchase-orders", get(orders).post(create_order))
        .route("/purchase-orders/{id}", get(order_detail).put(update_order))
        .route("/purchase-orders/{id}/confirm", post(confirm_order))
        .route("/purchase-orders/{id}/cancel", post(cancel_order))
}

fn text(value: &str, min: usize, max: usize, label: &str, multiline: bool) -> AppResult<String> {
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count())
        || value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(AppError::bad_request(format!(
            "{label} must contain {min}–{max} characters without unsupported control characters"
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(
    value: Option<&str>,
    max: usize,
    label: &str,
    multiline: bool,
) -> AppResult<Option<String>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| text(value, 1, max, label, multiline))
        .transpose()
}

fn expected_date(value: Option<NaiveDate>) -> AppResult<Option<NaiveDate>> {
    if value.is_some_and(|date| !(1..=9999).contains(&date.year())) {
        return Err(AppError::bad_request(
            "Tanggal perkiraan harus berada antara 0001-01-01 dan 9999-12-31.",
        ));
    }
    Ok(value)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SupplierInput {
    code: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SupplierUpdate {
    code: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    active: bool,
    version: i32,
    idempotency_key: Uuid,
}

struct SupplierFields {
    code: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
}

fn supplier_fields(
    code: &str,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
) -> AppResult<SupplierFields> {
    Ok(SupplierFields {
        code: text(&code.to_uppercase(), 1, 64, "Supplier code", false)?,
        name: text(name, 1, 200, "Supplier name", false)?,
        email: optional_text(email, 254, "Email", false)?,
        phone: optional_text(phone, 64, "Phone", false)?,
        address: optional_text(address, 1000, "Address", true)?,
    })
}

async fn suppliers(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY s.name,s.id),'[]'::jsonb) FROM suppliers s",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({"items":items})))
}

async fn create_supplier(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<SupplierInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("supplier.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let fields = supplier_fields(
        &body.code,
        &body.name,
        body.email.as_deref(),
        body.phone.as_deref(),
        body.address.as_deref(),
    )?;
    let id = Uuid::new_v4();
    let result: Value = sqlx::query_scalar(
        "INSERT INTO suppliers(id,code,name,email,phone,address) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(suppliers)",
    ).bind(id).bind(fields.code).bind(fields.name).bind(fields.email).bind(fields.phone).bind(fields.address)
        .fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "supplier.created",
        "supplier",
        id,
        json!({"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn update_supplier(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SupplierUpdate>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("supplier.update", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let before: Value =
        sqlx::query_scalar("SELECT to_jsonb(s) FROM suppliers s WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    check_version(body.version, before["version"].as_i64().unwrap() as i32)?;
    let fields = supplier_fields(
        &body.code,
        &body.name,
        body.email.as_deref(),
        body.phone.as_deref(),
        body.address.as_deref(),
    )?;
    let result: Value = sqlx::query_scalar(
        "UPDATE suppliers SET code=$2,name=$3,email=$4,phone=$5,address=$6,active=$7,version=version+1,updated_at=now() WHERE id=$1 RETURNING to_jsonb(suppliers)",
    ).bind(id).bind(fields.code).bind(fields.name).bind(fields.email).bind(fields.phone).bind(fields.address).bind(body.active)
        .fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "supplier.updated",
        "supplier",
        id,
        json!({"before":before,"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

const ORDER_JSON: &str = r#"jsonb_build_object(
    'id',o.id,'number',o.number,'supplier_id',o.supplier_id,'supplier_code',o.supplier_code,'supplier_name',o.supplier_name,
    'status',o.status,'expected_date',o.expected_date,'notes',o.notes,'total',o.total::text,'version',o.version,
    'created_at',o.created_at,'updated_at',o.updated_at,'confirmed_at',o.confirmed_at,'cancelled_at',o.cancelled_at,
    'cancellation_reason',o.cancellation_reason,'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
    'id',l.id,'product_id',l.product_id,'product_sku',l.product_sku,'product_name',l.product_name,'unit',l.unit,
    'quantity',l.quantity::text,'unit_cost',l.unit_cost::text,'total',l.total::text) ORDER BY l.position)
    FROM purchase_order_lines l WHERE l.order_id=o.id),'[]'::jsonb))"#;

fn order_select() -> String {
    format!("SELECT {ORDER_JSON} FROM purchase_orders o WHERE o.id=$1")
}

async fn read_order(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Value> {
    Ok(sqlx::query_scalar(&order_select())
        .bind(id)
        .fetch_one(&mut **tx)
        .await?)
}

async fn orders(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    // A single SQL statement keeps header and line snapshots consistent while
    // another session edits a draft.
    let sql = format!(
        "SELECT COALESCE(jsonb_agg(x.item ORDER BY x.created_at DESC,x.id),'[]'::jsonb) FROM (SELECT o.created_at,o.id,{ORDER_JSON} item FROM purchase_orders o ORDER BY o.created_at DESC,o.id LIMIT 500) x"
    );
    let items: Value = sqlx::query_scalar(&sql).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

async fn order_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(&order_select())
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(AppError::not_found)?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OrderLineInput {
    product_id: Uuid,
    quantity: String,
    unit_cost: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OrderInput {
    supplier_id: Uuid,
    expected_date: Option<NaiveDate>,
    notes: Option<String>,
    items: Vec<OrderLineInput>,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OrderUpdate {
    supplier_id: Uuid,
    expected_date: Option<NaiveDate>,
    notes: Option<String>,
    items: Vec<OrderLineInput>,
    version: i32,
    idempotency_key: Uuid,
}

struct LineSnapshot {
    product_id: Uuid,
    sku: String,
    name: String,
    unit: String,
    quantity: Decimal,
    cost: Decimal,
    total: Decimal,
}

async fn active_supplier(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<(String, String)> {
    let supplier = sqlx::query("SELECT code,name,active FROM suppliers WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::bad_request("Supplier not found"))?;
    if !supplier.get::<bool, _>("active") {
        return Err(AppError::conflict(
            "Supplier is inactive; select an active supplier",
        ));
    }
    Ok((supplier.get("code"), supplier.get("name")))
}

async fn snapshot_lines(
    tx: &mut Transaction<'_, Postgres>,
    items: &[OrderLineInput],
) -> AppResult<(Vec<LineSnapshot>, Decimal)> {
    if items.is_empty() || items.len() > 100 {
        return Err(AppError::bad_request(
            "Purchase order must contain 1–100 items",
        ));
    }
    let mut seen = BTreeSet::new();
    for item in items {
        if !seen.insert(item.product_id) {
            return Err(AppError::bad_request(
                "Combine duplicate products into one purchase order line",
            ));
        }
    }
    let ids: Vec<Uuid> = seen.into_iter().collect();
    // Master-data edits/deletions and stock operations use this same product
    // lock. Multiple lines are always locked in UUID order.
    sqlx::query("SELECT id FROM products WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let mut lines = Vec::with_capacity(items.len());
    let mut total = Decimal::ZERO;
    for item in items {
        let quantity =
            domain::decimal(&item.quantity, 3, "Quantity").map_err(AppError::bad_request)?;
        let cost =
            domain::decimal(&item.unit_cost, 4, "Unit cost").map_err(AppError::bad_request)?;
        let line_total = domain::money(quantity * cost);
        if line_total <= Decimal::ZERO {
            return Err(AppError::bad_request(
                "Each purchase order line total must be at least 0.01",
            ));
        }
        total += line_total;
        if total > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request("Purchase order value limit exceeded"));
        }
        let product = sqlx::query("SELECT sku,name,unit,kind FROM products WHERE id=$1")
            .bind(item.product_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Product not found"))?;
        if product.get::<String, _>("kind") != "stock" {
            return Err(AppError::bad_request(
                "Purchase orders currently support stock products only",
            ));
        }
        lines.push(LineSnapshot {
            product_id: item.product_id,
            sku: product.get("sku"),
            name: product.get("name"),
            unit: product.get("unit"),
            quantity,
            cost,
            total: line_total,
        });
    }
    Ok((lines, total))
}

async fn insert_lines(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    lines: Vec<LineSnapshot>,
) -> AppResult<()> {
    for (index, line) in lines.into_iter().enumerate() {
        sqlx::query("INSERT INTO purchase_order_lines(id,order_id,position,product_id,product_sku,product_name,unit,quantity,unit_cost,total) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(Uuid::new_v4()).bind(id).bind((index+1) as i32).bind(line.product_id).bind(line.sku).bind(line.name).bind(line.unit).bind(line.quantity).bind(line.cost).bind(line.total)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

fn check_version(expected: i32, current: i32) -> AppResult<()> {
    if expected < 1 {
        return Err(AppError::bad_request("Version must be positive"));
    }
    if expected != current {
        return Err(AppError::conflict(
            "Data changed in another session; refresh before continuing",
        ));
    }
    Ok(())
}

async fn create_order(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<OrderInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("purchase_order.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let notes = optional_text(body.notes.as_deref(), 2000, "Notes", true)?;
    let expected_date = expected_date(body.expected_date)?;
    let (supplier_code, supplier_name) = active_supplier(&mut tx, body.supplier_id).await?;
    let (lines, total) = snapshot_lines(&mut tx, &body.items).await?;
    let id = Uuid::new_v4();
    let sequence: i64 = sqlx::query_scalar("SELECT nextval('purchase_order_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("PO-{sequence:06}");
    sqlx::query("INSERT INTO purchase_orders(id,number,supplier_id,supplier_code,supplier_name,expected_date,notes,total,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(id).bind(number).bind(body.supplier_id).bind(supplier_code).bind(supplier_name).bind(expected_date).bind(notes).bind(total).bind(user.id)
        .execute(&mut *tx).await?;
    insert_lines(&mut tx, id, lines).await?;
    let result = read_order(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "purchase_order.created",
        "purchase_order",
        id,
        json!({"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn update_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<OrderUpdate>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload(
        "purchase_order.update",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let order = lock_order(&mut tx, id, body.version).await?;
    if order.get::<String, _>("status") != "draft" {
        return Err(AppError::conflict(
            "Only draft purchase orders can be edited",
        ));
    }
    let before = read_order(&mut tx, id).await?;
    let notes = optional_text(body.notes.as_deref(), 2000, "Notes", true)?;
    let expected_date = expected_date(body.expected_date)?;
    let (supplier_code, supplier_name) = active_supplier(&mut tx, body.supplier_id).await?;
    let (lines, total) = snapshot_lines(&mut tx, &body.items).await?;
    sqlx::query("UPDATE purchase_orders SET supplier_id=$2,supplier_code=$3,supplier_name=$4,expected_date=$5,notes=$6,total=$7,version=version+1,updated_at=now() WHERE id=$1")
        .bind(id).bind(body.supplier_id).bind(supplier_code).bind(supplier_name).bind(expected_date).bind(notes).bind(total)
        .execute(&mut *tx).await?;
    sqlx::query("DELETE FROM purchase_order_lines WHERE order_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_lines(&mut tx, id, lines).await?;
    let result = read_order(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "purchase_order.updated",
        "purchase_order",
        id,
        json!({"before":before,"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn lock_order(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: i32,
) -> AppResult<sqlx::postgres::PgRow> {
    let row = sqlx::query(
        "SELECT status,version,supplier_id FROM purchase_orders WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(AppError::not_found)?;
    check_version(version, row.get("version"))?;
    Ok(row)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionInput {
    version: i32,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CancelInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

async fn confirm_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload(
        "purchase_order.confirm",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let row = lock_order(&mut tx, id, body.version).await?;
    if row.get::<String, _>("status") != "draft" {
        return Err(AppError::conflict(
            "Only draft purchase orders can be confirmed",
        ));
    }
    active_supplier(&mut tx, row.get("supplier_id")).await?;
    let products = sqlx::query("SELECT p.id,p.kind FROM products p WHERE p.id IN (SELECT product_id FROM purchase_order_lines WHERE order_id=$1) ORDER BY p.id FOR UPDATE")
        .bind(id).fetch_all(&mut *tx).await?;
    if products.is_empty()
        || products
            .iter()
            .any(|row| row.get::<String, _>("kind") != "stock")
    {
        return Err(AppError::conflict("Purchase order requires stock products"));
    }
    // Confirmation preserves the reviewed draft's supplier and item snapshots.
    sqlx::query("UPDATE purchase_orders SET status='confirmed',confirmed_at=now(),updated_at=now(),version=version+1 WHERE id=$1")
        .bind(id).execute(&mut *tx).await?;
    let result = read_order(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "purchase_order.confirmed",
        "purchase_order",
        id,
        json!({"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn cancel_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CancelInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload(
        "purchase_order.cancel",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let reason = text(&body.reason, 3, 1000, "Cancellation reason", true)?;
    let row = lock_order(&mut tx, id, body.version).await?;
    if row.get::<String, _>("status") == "cancelled" {
        return Err(AppError::conflict("Purchase order is already cancelled"));
    }
    sqlx::query("UPDATE purchase_orders SET status='cancelled',cancelled_at=now(),cancellation_reason=$2,updated_at=now(),version=version+1 WHERE id=$1")
        .bind(id).bind(reason).execute(&mut *tx).await?;
    let result = read_order(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "purchase_order.cancelled",
        "purchase_order",
        id,
        json!({"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
