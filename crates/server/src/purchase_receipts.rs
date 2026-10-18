use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
    purchasing::{check_version, read_order},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/purchase-orders/{id}/receive", post(receive))
        .route("/purchase-receipts/{id}/reverse", post(reverse))
        .route("/purchasing/summary", get(summary))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiveInput {
    version: i32,
    idempotency_key: Uuid,
    reference: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

fn clean_text(value: &str, min: usize, max: usize, multiline: bool) -> AppResult<String> {
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count())
        || value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(AppError::bad_request(format!(
            "Text must contain {min}–{max} characters without unsupported control characters"
        )));
    }
    Ok(value.to_owned())
}

async fn lock_order(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: i32,
) -> AppResult<sqlx::postgres::PgRow> {
    let order = sqlx::query("SELECT status,version,number,total,active_receipt_id FROM purchase_orders WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut **tx).await?.ok_or_else(AppError::not_found)?;
    check_version(version, order.get("version"))?;
    Ok(order)
}

async fn receive(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReceiveInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let reference = body
        .reference
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| clean_text(value, 1, 200, false))
        .transpose()?;
    let request = payload(
        "purchase_order.receive",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let order = lock_order(&mut tx, id, body.version).await?;
    if order.get::<String, _>("status") != "confirmed"
        || order.get::<Option<Uuid>, _>("active_receipt_id").is_some()
    {
        return Err(AppError::conflict(
            "Only confirmed purchase orders without an active receipt can be received",
        ));
    }
    // The confirmed supplier snapshot remains valid even if that supplier was
    // subsequently made inactive. Stock products are locked in UUID order.
    sqlx::query("SELECT id FROM products WHERE id IN (SELECT product_id FROM purchase_order_lines WHERE order_id=$1) ORDER BY id FOR UPDATE")
        .bind(id).fetch_all(&mut *tx).await?;
    let lines = sqlx::query("SELECT l.*,p.kind,p.unit AS stock_unit,p.quantity AS stock_quantity,p.average_cost AS stock_average,p.inventory_value AS stock_value FROM purchase_order_lines l JOIN products p ON p.id=l.product_id WHERE l.order_id=$1 ORDER BY l.position")
        .bind(id).fetch_all(&mut *tx).await?;
    if lines.is_empty() || lines.len() > 100 {
        return Err(AppError::conflict(
            "Purchase order requires 1–100 stock products",
        ));
    }
    let mut total = Decimal::ZERO;
    // Validate the whole receipt before writing any of its lines.
    for line in &lines {
        if line.get::<String, _>("kind") != "stock" {
            return Err(AppError::conflict("Purchase order requires stock products"));
        }
        if line.get::<String, _>("unit") != line.get::<String, _>("stock_unit") {
            return Err(AppError::conflict(format!(
                "Satuan produk {} pada PO ({}) berbeda dari katalog saat ini ({}). Periksa satuan katalog dan PO sebelum menerima barang; jumlah tidak dikonversi otomatis.",
                line.get::<String, _>("product_sku"),
                line.get::<String, _>("unit"),
                line.get::<String, _>("stock_unit")
            )));
        }
        let quantity =
            line.get::<Decimal, _>("stock_quantity") + line.get::<Decimal, _>("quantity");
        let value = line.get::<Decimal, _>("stock_value") + line.get::<Decimal, _>("total");
        if quantity > Decimal::from(1_000_000_000u64) {
            return Err(AppError::bad_request("Stock quantity limit exceeded"));
        }
        if value > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request(
                "Product inventory value limit exceeded",
            ));
        }
        total += line.get::<Decimal, _>("total");
    }
    if total != order.get::<Decimal, _>("total") || total <= Decimal::ZERO {
        return Err(AppError::conflict(
            "Purchase order total does not match its lines",
        ));
    }
    let receipt_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let sequence: i64 = sqlx::query_scalar("SELECT nextval('purchase_receipt_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("GR-{sequence:06}");
    let accrual: String = sqlx::query_scalar(
        "SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'",
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'purchase_receipt',$3,$4)")
        .bind(journal_id).bind(format!("Goods receipt {number} / {}", order.get::<String,_>("number")))
        .bind(receipt_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1300',$3,0),($4,$2,$5,0,$3)")
        .bind(Uuid::new_v4()).bind(journal_id).bind(total).bind(Uuid::new_v4()).bind(accrual)
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO purchase_receipts(id,number,order_id,reference,total,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(receipt_id).bind(&number).bind(id).bind(&reference).bind(total).bind(journal_id).bind(user.id)
        .execute(&mut *tx).await?;
    for line in lines {
        let product_id: Uuid = line.get("product_id");
        let quantity: Decimal = line.get("quantity");
        let value: Decimal = line.get("total");
        let before_quantity: Decimal = line.get("stock_quantity");
        let before_value: Decimal = line.get("stock_value");
        let before_average: Decimal = line.get("stock_average");
        let new_quantity = before_quantity + quantity;
        let new_value = before_value + value;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(new_quantity)
        .bind(new_value)
        .bind(domain::average(new_value, new_quantity))
        .execute(&mut *tx)
        .await?;
        let movement = Uuid::new_v4();
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,purchase_receipt_id) VALUES($1,$2,'purchase_receipt',$3,$4,$5,$6,$7,$8)")
            .bind(movement).bind(product_id).bind(quantity).bind(line.get::<Decimal,_>("unit_cost"))
            .bind(value).bind(&reference).bind(user.id).bind(receipt_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO purchase_receipt_lines(id,receipt_id,position,movement_id,product_id,product_sku,product_name,unit,quantity,unit_cost,value,before_quantity,before_average_cost,before_inventory_value) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
            .bind(Uuid::new_v4()).bind(receipt_id).bind(line.get::<i32,_>("position")).bind(movement)
            .bind(product_id).bind(line.get::<String,_>("product_sku")).bind(line.get::<String,_>("product_name"))
            .bind(line.get::<String,_>("unit")).bind(quantity).bind(line.get::<Decimal,_>("unit_cost"))
            .bind(value).bind(before_quantity).bind(before_average).bind(before_value).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE purchase_orders SET status='received',active_receipt_id=$2,received_at=now(),updated_at=now(),version=version+1 WHERE id=$1")
        .bind(id).bind(receipt_id).execute(&mut *tx).await?;
    let result = read_order(&mut tx, id).await?;
    audit(&mut tx,&user,"purchase_order.received","purchase_order",id,
        json!({"receipt_id":receipt_id,"number":number,"journal_id":journal_id,"total":total.to_string(),"after":result})).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let reason = clean_text(&body.reason, 3, 1000, true)?;
    let request = payload(
        "purchase_receipt.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let receipt =
        sqlx::query("SELECT order_id,journal_id,number FROM purchase_receipts WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    let order_id: Uuid = receipt.get("order_id");
    let order = lock_order(&mut tx, order_id, body.version).await?;
    if order.get::<String, _>("status") != "received"
        || order.get::<Option<Uuid>, _>("active_receipt_id") != Some(id)
    {
        return Err(AppError::conflict(
            "Only the active goods receipt of a received purchase order can be reversed",
        ));
    }
    sqlx::query("SELECT id FROM products WHERE id IN (SELECT product_id FROM purchase_receipt_lines WHERE receipt_id=$1) ORDER BY id FOR UPDATE")
        .bind(id).fetch_all(&mut *tx).await?;
    let blocked: Option<String> = sqlx::query_scalar("SELECT purchase_receipt_blocked_reason($1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(reason) = blocked {
        return Err(AppError::conflict(reason));
    }
    let lines =
        sqlx::query("SELECT * FROM purchase_receipt_lines WHERE receipt_id=$1 ORDER BY position")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    let reversal_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'purchase_receipt',$2,$3,$4)")
        .bind(reversal_id).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal_id).bind(format!("Reversal of goods receipt {}",receipt.get::<String,_>("number")))
        .bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    let journal_lines = sqlx::query(
        "SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY id",
    )
    .bind(receipt.get::<Uuid, _>("journal_id"))
    .fetch_all(&mut *tx)
    .await?;
    for line in journal_lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(journal_id).bind(line.get::<String,_>("account_code"))
            .bind(line.get::<Decimal,_>("credit")).bind(line.get::<Decimal,_>("debit"))
            .execute(&mut *tx).await?;
    }
    for line in lines {
        let product_id: Uuid = line.get("product_id");
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(line.get::<Decimal, _>("before_quantity"))
        .bind(line.get::<Decimal, _>("before_inventory_value"))
        .bind(line.get::<Decimal, _>("before_average_cost"))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,purchase_receipt_id,reverses_id) VALUES($1,$2,'purchase_receipt_reversal',$3,$4,$5,$6,$7,$8,$9)")
            .bind(Uuid::new_v4()).bind(product_id).bind(-line.get::<Decimal,_>("quantity"))
            .bind(line.get::<Decimal,_>("unit_cost")).bind(line.get::<Decimal,_>("value"))
            .bind(&reason).bind(user.id).bind(id).bind(line.get::<Uuid,_>("movement_id"))
            .execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE purchase_orders SET status='confirmed',active_receipt_id=NULL,received_at=NULL,updated_at=now(),version=version+1 WHERE id=$1")
        .bind(order_id).execute(&mut *tx).await?;
    let result = read_order(&mut tx, order_id).await?;
    audit(&mut tx,&user,"purchase_receipt.reversed","purchase_receipt",id,
        json!({"order_id":order_id,"reversal_id":reversal_id,"journal_id":journal_id,"reason":reason,"average_policy":"exact_snapshot","after":result})).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn summary(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(r#"
        WITH status_totals AS (
            SELECT status,count(*) count,sum(total) value FROM purchase_orders GROUP BY status
        )
        SELECT jsonb_build_object(
            'counts',jsonb_build_object('draft',COALESCE((SELECT count FROM status_totals WHERE status='draft'),0),
                'confirmed',COALESCE((SELECT count FROM status_totals WHERE status='confirmed'),0),
                'received',COALESCE((SELECT count FROM status_totals WHERE status='received'),0),
                'cancelled',COALESCE((SELECT count FROM status_totals WHERE status='cancelled'),0)),
            'values',jsonb_build_object('draft',COALESCE((SELECT value FROM status_totals WHERE status='draft'),0)::text,
                'confirmed',COALESCE((SELECT value FROM status_totals WHERE status='confirmed'),0)::text,
                'received',COALESCE((SELECT value FROM status_totals WHERE status='received'),0)::text,
                'cancelled',COALESCE((SELECT value FROM status_totals WHERE status='cancelled'),0)::text),
            'pending_receipt_total',COALESCE((SELECT value FROM status_totals WHERE status='confirmed'),0)::text,
            'received_unbilled_total',COALESCE((SELECT sum(r.total) FROM purchase_orders o
                JOIN purchase_receipts r ON r.id=o.active_receipt_id WHERE o.status='received'),0)::text,
            'as_of',now())
        "#).fetch_one(&state.db).await?;
    Ok(Json(result))
}
