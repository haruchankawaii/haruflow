use crate::{
    AppState,
    auth::AuthUser,
    business::{
        INVOICE_SELECT, ORDER_SELECT, audit, cached, journal, parse, payload, remember, text,
    },
    domain,
    error::{AppError, AppResult},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::str::FromStr;
use uuid::Uuid;

// All dated postings and period closure share this transaction lock. It is acquired
// before resource locks, so a close cannot race a posting into a closed period.
pub(crate) async fn check_open(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut **tx)
        .await?;
    let open:bool=sqlx::query_scalar("SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton")
        .fetch_one(&mut **tx).await?;
    if !open {
        return Err(AppError::conflict(
            "The current accounting period is closed",
        ));
    }
    Ok(())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounting/journals", post(manual_journal))
        .route("/accounting/period", get(period))
        .route("/accounting/period/close", post(close_period))
        .route("/inventory/adjustments", post(adjust_stock))
        .route("/orders/{id}/cancel", post(cancel_order))
        .route("/orders/{id}/reverse-shipment", post(reverse_shipment))
        .route("/invoices/{id}/reverse", post(reverse_invoice))
        .route("/invoices/{id}/payments", get(payments))
        .route(
            "/invoices/{id}/payments/{payment_id}/reverse",
            post(reverse_payment),
        )
}

fn reason(value: &str) -> AppResult<String> {
    let result = text(value, 1000, "Reason")?;
    if result.chars().count() < 3 {
        return Err(AppError::bad_request(
            "Reason must contain at least 3 characters",
        ));
    }
    Ok(result)
}

#[derive(Deserialize, Serialize)]
struct JournalLineInput {
    account_code: String,
    debit: String,
    credit: String,
}
#[derive(Deserialize, Serialize)]
struct JournalInput {
    description: String,
    lines: Vec<JournalLineInput>,
    idempotency_key: Uuid,
}

fn nonnegative_money(value: &str) -> AppResult<Decimal> {
    let amount = Decimal::from_str(value)
        .map_err(|_| AppError::bad_request("Journal values must be decimal strings"))?;
    if amount < Decimal::ZERO
        || amount > Decimal::from(1_000_000_000_000u64)
        || amount.normalize().scale() > 2
    {
        return Err(AppError::bad_request(
            "Journal values must be nonnegative with at most 2 decimal places and at most 1000000000000",
        ));
    }
    Ok(amount)
}

async fn manual_journal(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<JournalInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("journal.manual", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let description = text(&body.description, 500, "Description")?;
    if body.lines.len() < 2 || body.lines.len() > 100 {
        return Err(AppError::bad_request("Journal must contain 2–100 lines"));
    }
    let mut lines = Vec::new();
    let mut debit_total = Decimal::ZERO;
    let mut credit_total = Decimal::ZERO;
    for line in &body.lines {
        if line.account_code == "1200" || line.account_code == "1300" {
            return Err(AppError::bad_request(
                "Receivables and inventory are control accounts; use invoice or stock workflows",
            ));
        }
        let debit = nonnegative_money(&line.debit)?;
        let credit = nonnegative_money(&line.credit)?;
        if (debit > Decimal::ZERO) == (credit > Decimal::ZERO) {
            return Err(AppError::bad_request(
                "Each line must contain exactly one positive debit or credit",
            ));
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE code=$1)")
                .bind(&line.account_code)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::bad_request("Unknown account code"));
        }
        debit_total += debit;
        credit_total += credit;
        lines.push((&line.account_code, debit, credit));
    }
    if debit_total != credit_total || debit_total > Decimal::from(1_000_000_000_000u64) {
        return Err(AppError::bad_request(
            "Journal debits and credits must balance and total at most 1000000000000",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'manual',$1,$3)")
        .bind(id).bind(&description).bind(user.id).execute(&mut *tx).await?;
    for (account, debit, credit) in lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(id).bind(account).bind(debit).bind(credit).execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        &user,
        "journal.posted",
        "journal_entry",
        id,
        json!({"description":description,"total":debit_total.to_string()}),
    )
    .await?;
    let result = json!({"id":id,"description":description,"total_debit":debit_total.to_string(),"total_credit":credit_total.to_string()});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn period(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let result:Value=sqlx::query_scalar("SELECT jsonb_build_object('closed_through',closed_through,'closed_at',closed_at,'reason',reason) FROM accounting_period WHERE singleton")
        .fetch_one(&state.db).await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
struct CloseInput {
    through: String,
    reason: String,
    idempotency_key: Uuid,
}

async fn close_period(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CloseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("period.close", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    let through = NaiveDate::parse_from_str(&body.through, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Close date must use YYYY-MM-DD"))?;
    let reason = reason(&body.reason)?;
    let today: NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE (SELECT timezone FROM company))::date")
            .fetch_one(&mut *tx)
            .await?;
    if through > today {
        return Err(AppError::bad_request(
            "Cannot close a future accounting period",
        ));
    }
    let previous: Option<NaiveDate> = sqlx::query_scalar(
        "SELECT closed_through FROM accounting_period WHERE singleton FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    if previous.is_some_and(|date| through <= date) {
        return Err(AppError::conflict(
            "Close date must be after the previous closed period",
        ));
    }
    sqlx::query("UPDATE accounting_period SET closed_through=$1,closed_by=$2,reason=$3,closed_at=now() WHERE singleton")
        .bind(through).bind(user.id).bind(&reason).execute(&mut *tx).await?;
    let event_id = Uuid::new_v4();
    audit(
        &mut tx,
        &user,
        "period.closed",
        "accounting_period",
        event_id,
        json!({"through":through,"reason":reason}),
    )
    .await?;
    let result = json!({"closed_through":through,"reason":reason});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
struct AdjustmentInput {
    product_id: Uuid,
    quantity: String,
    unit_cost: Option<String>,
    reason: String,
    idempotency_key: Uuid,
}

async fn adjust_stock(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AdjustmentInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("inventory.adjust", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let quantity = Decimal::from_str(&body.quantity)
        .map_err(|_| AppError::bad_request("Quantity must be a signed decimal string"))?;
    let magnitude = parse(&quantity.abs().to_string(), 3, "Quantity")?;
    let reason = reason(&body.reason)?;
    let product = sqlx::query(
        "SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(body.product_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(AppError::not_found)?;
    let on_hand: Decimal = product.get("quantity");
    let carrying: Decimal = product.get("inventory_value");
    let old_average: Decimal = product.get("average_cost");
    let (unit_cost, value, new_quantity, new_value, new_average) = if quantity > Decimal::ZERO {
        let unit_cost = parse(
            body.unit_cost.as_deref().ok_or_else(|| {
                AppError::bad_request("Unit cost is required for a positive adjustment")
            })?,
            4,
            "Unit cost",
        )?;
        let value = domain::money(magnitude * unit_cost);
        if value <= Decimal::ZERO {
            return Err(AppError::bad_request(
                "Adjustment value must be at least 0.01",
            ));
        }
        let new_quantity = on_hand + magnitude;
        let new_value = carrying + value;
        (
            unit_cost,
            value,
            new_quantity,
            new_value,
            domain::average(new_value, new_quantity),
        )
    } else {
        if magnitude > on_hand {
            return Err(AppError::conflict("Adjustment exceeds available stock"));
        }
        let value = domain::shipment_cost(magnitude, on_hand, old_average, carrying);
        let remaining = on_hand - magnitude;
        (
            old_average,
            value,
            remaining,
            carrying - value,
            if remaining == Decimal::ZERO {
                Decimal::ZERO
            } else {
                old_average
            },
        )
    };
    if new_quantity > Decimal::from(1_000_000_000u64)
        || new_value > Decimal::from(1_000_000_000_000u64)
    {
        return Err(AppError::bad_request(
            "Inventory quantity or value limit exceeded",
        ));
    }
    sqlx::query("UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1")
        .bind(body.product_id)
        .bind(new_quantity)
        .bind(new_value)
        .bind(new_average)
        .execute(&mut *tx)
        .await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by) VALUES($1,$2,'adjustment',$3,$4,$5,$6,$7)")
        .bind(id).bind(body.product_id).bind(quantity).bind(unit_cost).bind(value).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    let (debit, credit) = if quantity > Decimal::ZERO {
        ("1300", "3000")
    } else {
        ("3000", "1300")
    };
    journal(
        &mut tx,
        &user,
        "adjustment",
        id,
        &format!("Stock adjustment: {reason}"),
        (debit, credit),
        value,
    )
    .await?;
    audit(
        &mut tx,
        &user,
        "inventory.adjusted",
        "inventory_movement",
        id,
        json!({"reason":reason,"quantity":quantity.to_string(),"product_id":body.product_id}),
    )
    .await?;
    let result = json!({"id":id,"product_id":body.product_id,"quantity":quantity.to_string(),"value":value.to_string()});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
struct ReasonInput {
    reason: String,
    idempotency_key: Uuid,
}

async fn cancel_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonInput>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let request = payload("order.cancel", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let reason = reason(&body.reason)?;
    let status: String =
        sqlx::query_scalar("SELECT status FROM sales_orders WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    if status != "draft" && status != "confirmed" {
        return Err(AppError::conflict(
            "Only draft or confirmed orders can be cancelled",
        ));
    }
    sqlx::query("UPDATE sales_orders SET status='cancelled' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "order.cancelled",
        "sales_order",
        id,
        json!({"reason":reason}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn payments(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',p.id,'amount',p.amount::text,
        'reference',p.reference,'created_at',p.created_at,'reversed',r.id IS NOT NULL,'reversal_reason',r.reason)
        ORDER BY p.created_at DESC),'[]'::jsonb) FROM payments p LEFT JOIN posting_reversals r ON r.source_type='payment' AND r.source_id=p.id
        WHERE p.invoice_id=$1"#).bind(id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

async fn reversal_record(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    kind: &str,
    source: Uuid,
    reason: &str,
) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,$2,$3,$4,$5)")
        .bind(id).bind(kind).bind(source).bind(reason).bind(user.id).execute(&mut **tx).await?;
    Ok(id)
}

async fn reverse_payment(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, payment_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ReasonInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload(
        "payment.reverse",
        &user,
        &json!({"id":id,"payment_id":payment_id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = reason(&body.reason)?;
    let invoice =
        sqlx::query("SELECT total,paid,status,number FROM invoices WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    if invoice.get::<String, _>("status") == "void" {
        return Err(AppError::conflict("Invoice is already reversed"));
    }
    let payment = sqlx::query("SELECT amount FROM payments WHERE id=$1 AND invoice_id=$2")
        .bind(payment_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let reversed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='payment' AND source_id=$1)").bind(payment_id).fetch_one(&mut *tx).await?;
    if reversed {
        return Err(AppError::conflict("Payment is already reversed"));
    }
    let amount: Decimal = payment.get("amount");
    let paid: Decimal = invoice.get("paid");
    let new_paid = paid - amount;
    if new_paid < Decimal::ZERO {
        return Err(AppError::conflict(
            "Payment balance does not permit this reversal",
        ));
    }
    let reversal_id = reversal_record(&mut tx, &user, "payment", payment_id, &reason).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        reversal_id,
        &format!("Reverse payment: {reason}"),
        ("1200", "1100"),
        amount,
    )
    .await?;
    sqlx::query("UPDATE invoices SET paid=$2,status=$3 WHERE id=$1")
        .bind(id)
        .bind(new_paid)
        .bind(if new_paid == Decimal::ZERO {
            "unpaid"
        } else {
            "partial"
        })
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "payment.reversed",
        "payment",
        payment_id,
        json!({"reason":reason,"reversal_id":reversal_id}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse_invoice(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("invoice.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = reason(&body.reason)?;
    let order_id: Uuid = sqlx::query_scalar("SELECT order_id FROM invoices WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    sqlx::query("SELECT id FROM sales_orders WHERE id=$1 FOR UPDATE")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    let invoice = sqlx::query("SELECT total,paid,status FROM invoices WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if invoice.get::<String, _>("status") == "void" {
        return Err(AppError::conflict("Invoice is already reversed"));
    }
    if invoice.get::<Decimal, _>("paid") != Decimal::ZERO {
        return Err(AppError::conflict(
            "Reverse all payments before reversing the invoice",
        ));
    }
    let reversal_id = reversal_record(&mut tx, &user, "invoice", id, &reason).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        reversal_id,
        &format!("Reverse invoice: {reason}"),
        ("4000", "1200"),
        invoice.get("total"),
    )
    .await?;
    sqlx::query("UPDATE invoices SET status='void' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sales_orders SET status='shipped',invoiced_at=NULL WHERE id=$1")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "invoice.reversed",
        "invoice",
        id,
        json!({"reason":reason,"reversal_id":reversal_id}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse_shipment(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("shipment.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = reason(&body.reason)?;
    let status: String =
        sqlx::query_scalar("SELECT status FROM sales_orders WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    if status != "shipped" {
        return Err(AppError::conflict(
            "Only a shipped order without an active invoice can have its shipment reversed",
        ));
    }
    let active_invoice: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invoices WHERE order_id=$1 AND status<>'void')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if active_invoice {
        return Err(AppError::conflict(
            "Reverse the invoice before reversing its shipment",
        ));
    }
    let posting_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM shipment_postings WHERE order_id=$1 AND NOT reversed FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::conflict("No active shipment found"))?;
    let movements=sqlx::query("SELECT id,product_id,quantity,unit_cost,value FROM inventory_movements WHERE shipment_posting_id=$1 AND kind='shipment' ORDER BY product_id")
        .bind(posting_id).fetch_all(&mut *tx).await?;
    if movements.is_empty() {
        return Err(AppError::conflict("Shipment has no stock movements"));
    }
    let reversal_id = reversal_record(&mut tx, &user, "shipment", posting_id, &reason).await?;
    let mut total = Decimal::ZERO;
    for movement in movements {
        let product_id: Uuid = movement.get("product_id");
        let quantity = -movement.get::<Decimal, _>("quantity");
        let value: Decimal = movement.get("value");
        let product =
            sqlx::query("SELECT quantity,inventory_value FROM products WHERE id=$1 FOR UPDATE")
                .bind(product_id)
                .fetch_one(&mut *tx)
                .await?;
        let new_quantity = product.get::<Decimal, _>("quantity") + quantity;
        let new_value = product.get::<Decimal, _>("inventory_value") + value;
        if new_quantity > Decimal::from(1_000_000_000u64)
            || new_value > Decimal::from(1_000_000_000_000u64)
        {
            return Err(AppError::bad_request(
                "Inventory limit would be exceeded by reversal",
            ));
        }
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(new_quantity)
        .bind(new_value)
        .bind(domain::average(new_value, new_quantity))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,reverses_id,shipment_posting_id) VALUES($1,$2,'shipment_reversal',$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(Uuid::new_v4()).bind(product_id).bind(quantity).bind(movement.get::<Decimal,_>("unit_cost")).bind(value).bind(&reason)
            .bind(id).bind(user.id).bind(movement.get::<Uuid,_>("id")).bind(posting_id).execute(&mut *tx).await?;
        total += value;
    }
    journal(
        &mut tx,
        &user,
        "reversal",
        reversal_id,
        &format!("Reverse shipment: {reason}"),
        ("1300", "5000"),
        total,
    )
    .await?;
    sqlx::query("UPDATE shipment_postings SET reversed=true WHERE id=$1")
        .bind(posting_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sales_orders SET status='confirmed',shipped_at=NULL WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "shipment.reversed",
        "sales_order",
        id,
        json!({"reason":reason,"reversal_id":reversal_id}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
