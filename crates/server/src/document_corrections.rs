use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember, text},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounting/journals/{id}/reverse", post(reverse_manual))
        .route("/inventory/receipts/{id}/reverse", post(reverse_receipt))
}

#[derive(Deserialize, Serialize)]
struct ReverseInput {
    reason: String,
    idempotency_key: Uuid,
}

fn reason(value: &str) -> AppResult<String> {
    let value = text(value, 1000, "Reason")?;
    if value.chars().count() < 3 {
        return Err(AppError::bad_request(
            "Reason must contain at least 3 characters",
        ));
    }
    Ok(value)
}

async fn record(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    source_type: &str,
    source_id: Uuid,
    reason: &str,
) -> AppResult<Uuid> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type=$1 AND source_id=$2)",
    )
    .bind(source_type)
    .bind(source_id)
    .fetch_one(&mut **tx)
    .await?;
    if exists {
        return Err(AppError::conflict(
            "This document has already been reversed",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,$2,$3,$4,$5)")
        .bind(id).bind(source_type).bind(source_id).bind(reason).bind(user.id).execute(&mut **tx).await?;
    Ok(id)
}

async fn reverse_manual(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("journal.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = reason(&body.reason)?;
    let original =
        sqlx::query("SELECT source_type,description FROM journal_entries WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Journal not found"))?;
    if original.get::<String, _>("source_type") != "manual" {
        return Err(AppError::bad_request(
            "Only manual journals can be reversed here; use the source document workflow",
        ));
    }
    let reversal = record(&mut tx, &user, "manual", id, &reason).await?;
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal_id).bind(format!("Reversal: {}", original.get::<String,_>("description")))
        .bind(reversal).bind(user.id).execute(&mut *tx).await?;
    let lines = sqlx::query(
        "SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for line in lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(journal_id).bind(line.get::<String,_>("account_code"))
            .bind(line.get::<Decimal,_>("credit")).bind(line.get::<Decimal,_>("debit"))
            .execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        &user,
        "journal.reversed",
        "journal_entry",
        id,
        json!({"reversal_id":reversal,"journal_id":journal_id,"reason":reason}),
    )
    .await?;
    let result = json!({"id":reversal,"journal_id":journal_id,"source_id":id,"reason":reason});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse_receipt(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("receipt.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = reason(&body.reason)?;
    let receipt = sqlx::query("SELECT product_id,kind,quantity,unit_cost,value,movement_seq,before_quantity,before_average_cost,before_inventory_value FROM inventory_movements WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| AppError::bad_request("Receipt not found"))?;
    if receipt.get::<String, _>("kind") != "receipt" {
        return Err(AppError::bad_request(
            "Only stock receipts can be reversed here",
        ));
    }
    let before_quantity = receipt.get::<Option<Decimal>,_>("before_quantity")
        .ok_or_else(|| AppError::conflict("Receipts recorded before this upgrade have no cost snapshot; use a stock adjustment"))?;
    let product_id: Uuid = receipt.get("product_id");
    let product = sqlx::query(
        "SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(product_id)
    .fetch_one(&mut *tx)
    .await?;
    let later: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inventory_movements WHERE product_id=$1 AND movement_seq>$2)",
    )
    .bind(product_id)
    .bind(receipt.get::<i64, _>("movement_seq"))
    .fetch_one(&mut *tx)
    .await?;
    if later {
        return Err(AppError::conflict(
            "This receipt has later stock movements; use a stock adjustment to preserve historical inventory costs",
        ));
    }
    let reversal = record(&mut tx, &user, "receipt", id, &reason).await?;
    let quantity: Decimal = receipt.get("quantity");
    let value: Decimal = receipt.get("value");
    let remaining_quantity = product.get::<Decimal, _>("quantity") - quantity;
    let remaining_value = product.get::<Decimal, _>("inventory_value") - value;
    if remaining_quantity < Decimal::ZERO
        || remaining_value < Decimal::ZERO
        || (remaining_quantity == Decimal::ZERO && remaining_value != Decimal::ZERO)
    {
        return Err(AppError::conflict(
            "Receipt cannot be removed from the current stock balance",
        ));
    }
    let before_value: Decimal = receipt.get("before_inventory_value");
    if remaining_quantity != before_quantity
        || remaining_value != before_value
        || product.get::<Decimal, _>("average_cost")
            != domain::average(before_value + value, before_quantity + quantity)
    {
        return Err(AppError::conflict(
            "Stock balance no longer matches the receipt snapshot",
        ));
    }
    let average = receipt.get::<Decimal, _>("before_average_cost");
    sqlx::query("UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1")
        .bind(product_id)
        .bind(remaining_quantity)
        .bind(remaining_value)
        .bind(average)
        .execute(&mut *tx)
        .await?;
    let movement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,reverses_id,created_by) VALUES($1,$2,'receipt_reversal',$3,$4,$5,$6,$7,$8)")
        .bind(movement_id).bind(product_id).bind(-quantity).bind(receipt.get::<Decimal,_>("unit_cost"))
        .bind(value).bind(&reason).bind(id).bind(user.id).execute(&mut *tx).await?;
    // A receipt always posts a positive value. Its exact inverse uses historical
    // carrying value, never the current weighted average.
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Reversal of stock receipt','reversal',$2,$3)")
        .bind(journal_id).bind(reversal).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'3000',$3,0),($4,$2,'1300',0,$3)")
        .bind(Uuid::new_v4()).bind(journal_id).bind(value).bind(Uuid::new_v4()).execute(&mut *tx).await?;
    audit(&mut tx, &user, "inventory.receipt_reversed", "inventory_movement", id,
        json!({"reversal_id":reversal,"journal_id":journal_id,"movement_id":movement_id,"product_id":product_id,"reason":reason,"average_policy":"exact_snapshot"})).await?;
    let result = json!({"id":reversal,"journal_id":journal_id,"source_id":id,"reason":reason});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
