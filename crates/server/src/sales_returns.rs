use crate::{
    AppState,
    auth::AuthUser,
    business::{INVOICE_SELECT, audit, cached, parse, payload, remember, text},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
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
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/invoices/{id}", get(invoice_detail))
        .route("/invoices/{id}/returns", post(create_return))
        .route("/sales-returns/{id}", get(return_detail))
        .route("/sales-returns/{id}/reverse", post(reverse_return))
}
async fn invoice_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result: Option<Value> = sqlx::query_scalar(INVOICE_SELECT)
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    Ok(Json(result.ok_or_else(AppError::not_found)?))
}
async fn return_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result: Option<Value> = sqlx::query_scalar("SELECT sales_return_document($1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(result.ok_or_else(AppError::not_found)?))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReturnItem {
    invoice_line_id: Uuid,
    quantity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    warehouse_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lot_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allocations: Vec<crate::inventory_extensions::StockSelection>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReturnInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cash_account_code: Option<String>,
    version: i64,
    reason: String,
    items: Vec<ReturnItem>,
    #[serde(default)]
    refund_confirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refund_mode: Option<String>,
    idempotency_key: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}

fn reason(input: &str) -> AppResult<String> {
    let reason = text(input, 1000, "Reason")?;
    if reason.chars().count() < 3 || reason.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "Reason must contain 3–1000 characters without control characters",
        ));
    }
    Ok(reason)
}
pub(crate) fn settlement_mode(mode: Option<&str>) -> AppResult<&str> {
    match mode.unwrap_or("immediate") {
        "immediate" => Ok("immediate"),
        "credit" => Ok("credit"),
        "deferred" => Ok("deferred"),
        _ => Err(AppError::bad_request(
            "Pilih penyelesaian immediate, credit, atau deferred.",
        )),
    }
}
fn allocated_value(
    total: Decimal,
    quantity: Decimal,
    previous_qty: Decimal,
    previous_value: Decimal,
    selected: Decimal,
) -> AppResult<Decimal> {
    let mut value = domain::money(total * (previous_qty + selected) / quantity) - previous_value;
    if previous_qty + selected < quantity {
        value = value.min((total - previous_value - Decimal::new(1, 2)).max(Decimal::ZERO));
    }
    if value <= Decimal::ZERO {
        return Err(AppError::conflict(
            "Selected quantity is too small to credit separately; combine it with the remaining quantity",
        ));
    }
    Ok(value)
}
async fn lock_invoice(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: i64,
) -> AppResult<sqlx::postgres::PgRow> {
    let order_id: Uuid = sqlx::query_scalar("SELECT order_id FROM invoices WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    sqlx::query("SELECT id FROM sales_orders WHERE id=$1 FOR UPDATE")
        .bind(order_id)
        .execute(&mut **tx)
        .await?;
    let invoice=sqlx::query("SELECT id,order_id,total,paid,credit_total,version,status,number FROM invoices WHERE id=$1 FOR UPDATE").bind(id).fetch_one(&mut **tx).await?;
    if invoice.get::<String, _>("status") == "void" {
        return Err(AppError::conflict("A void invoice cannot be returned"));
    }
    if invoice.get::<i64, _>("version") != version {
        return Err(AppError::conflict(
            "Invoice changed; reload before continuing",
        ));
    }
    Ok(invoice)
}
struct Source {
    allocation_id: Uuid,
    quantity: Decimal,
    cost: Decimal,
}
struct Line {
    invoice_line_id: Uuid,
    product_id: Uuid,
    sku: String,
    name: String,
    kind: String,
    quantity: Decimal,
    credit: Decimal,
    cost: Decimal,
    sources: Vec<Source>,
}
async fn create_return(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReturnInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mode = settlement_mode(body.refund_mode.as_deref())?;
    let reason = reason(&body.reason)?;
    if body.items.is_empty() || body.items.len() > 100 {
        return Err(AppError::bad_request("Choose 1–100 invoice lines"));
    }
    let mut selected = BTreeMap::new();
    for item in &body.items {
        if selected
            .insert(item.invoice_line_id, parse(&item.quantity, 3, "Quantity")?)
            .is_some()
        {
            return Err(AppError::bad_request("Combine duplicate invoice lines"));
        }
    }
    let request = payload("sales_return.create", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let invoice = lock_invoice(&mut tx, id, body.version).await?;
    let rows=sqlx::query("SELECT a.id,a.quantity,a.total,l.product_id,l.product_sku,l.product_name,l.product_kind,COALESCE((SELECT sum(r.quantity) FROM sales_return_lines r WHERE r.invoice_line_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=r.return_id)),0) returned_quantity,COALESCE((SELECT sum(r.credit) FROM sales_return_lines r WHERE r.invoice_line_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=r.return_id)),0) returned_credit FROM invoice_allocations a JOIN sales_order_lines l ON l.id=a.line_id WHERE a.invoice_id=$1 ORDER BY l.product_id")
        .bind(id).fetch_all(&mut *tx).await?;
    let product_ids: Vec<Uuid> = rows
        .iter()
        .filter(|row| selected.contains_key(&row.get::<Uuid, _>("id")))
        .map(|row| row.get("product_id"))
        .collect();
    sqlx::query("SELECT id FROM products WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&product_ids)
        .fetch_all(&mut *tx)
        .await?;
    let mut lines = Vec::new();
    let mut credit_total = Decimal::ZERO;
    let mut cost_total = Decimal::ZERO;
    for row in rows {
        let invoice_line_id: Uuid = row.get("id");
        let Some(quantity) = selected.remove(&invoice_line_id) else {
            continue;
        };
        let original_quantity: Decimal = row.get("quantity");
        let returned_quantity: Decimal = row.get("returned_quantity");
        let returned_credit: Decimal = row.get("returned_credit");
        if quantity > original_quantity - returned_quantity {
            return Err(AppError::conflict(
                "Return exceeds invoice quantity that has not already been returned",
            ));
        }
        let credit = allocated_value(
            row.get("total"),
            original_quantity,
            returned_quantity,
            returned_credit,
            quantity,
        )?;
        let kind: String = row.get("product_kind");
        let mut sources = Vec::new();
        let mut cost = Decimal::ZERO;
        if kind == "stock" {
            let deliveries=sqlx::query("SELECT a.id,a.product_unit,p.unit current_unit,x.quantity-COALESCE((SELECT sum(rs.quantity) FROM sales_return_sources rs JOIN sales_return_lines rl ON rl.id=rs.return_line_id WHERE rl.invoice_line_id=$1 AND rs.shipment_allocation_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=rl.return_id)),0) available,a.quantity,m.value,COALESCE((SELECT sum(rs.quantity) FROM sales_return_sources rs JOIN sales_return_lines rl ON rl.id=rs.return_line_id WHERE rs.shipment_allocation_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=rl.return_id)),0) returned_quantity,COALESCE((SELECT sum(rs.cost) FROM sales_return_sources rs JOIN sales_return_lines rl ON rl.id=rs.return_line_id WHERE rs.shipment_allocation_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=rl.return_id)),0) returned_cost FROM invoice_shipment_allocations x JOIN shipment_allocations a ON a.id=x.shipment_allocation_id JOIN shipment_postings s ON s.id=a.shipment_id JOIN inventory_movements m ON m.id=a.movement_id JOIN sales_order_lines sl ON sl.id=a.line_id JOIN products p ON p.id=sl.product_id WHERE x.invoice_allocation_id=$1 AND NOT s.reversed ORDER BY s.created_at,s.id")
                .bind(invoice_line_id).fetch_all(&mut *tx).await?;
            let mut remainder = quantity;
            for delivery in deliveries {
                let available: Decimal = delivery.get("available");
                let used = available.min(remainder);
                if used <= Decimal::ZERO {
                    continue;
                }
                let landed_cost_dependency: Option<String> = sqlx::query_scalar(
                    "SELECT landed_cost_movement_dependency(movement_id) FROM shipment_allocations WHERE id=$1",
                )
                .bind(delivery.get::<Uuid, _>("id"))
                .fetch_one(&mut *tx)
                .await?;
                if landed_cost_dependency.is_some() {
                    return Err(AppError::conflict(
                        "Balikkan alokasi biaya perolehan terkait sebelum menerima retur dari pengiriman historis ini.",
                    ));
                }
                if delivery.get::<String, _>("product_unit")
                    != delivery.get::<String, _>("current_unit")
                {
                    return Err(AppError::conflict(
                        "Product unit changed since shipment; restore the original unit before returning stock",
                    ));
                }
                let returned: Decimal = delivery.get("returned_quantity");
                let shipped: Decimal = delivery.get("quantity");
                let historical = (domain::money(
                    delivery.get::<Decimal, _>("value") * (returned + used) / shipped,
                ) - delivery.get::<Decimal, _>("returned_cost"))
                .max(Decimal::ZERO)
                .min(
                    delivery.get::<Decimal, _>("value")
                        - delivery.get::<Decimal, _>("returned_cost"),
                );
                cost += historical;
                sources.push(Source {
                    allocation_id: delivery.get("id"),
                    quantity: used,
                    cost: historical,
                });
                remainder -= used;
                if remainder == Decimal::ZERO {
                    break;
                }
            }
            if remainder != Decimal::ZERO {
                return Err(AppError::conflict(
                    "The invoice has no matching active delivery for this return",
                ));
            }
        }
        credit_total += credit;
        cost_total += cost;
        lines.push(Line {
            invoice_line_id,
            product_id: row.get("product_id"),
            sku: row.get("product_sku"),
            name: row.get("product_name"),
            kind,
            quantity,
            credit,
            cost,
            sources,
        });
    }
    if !selected.is_empty() {
        return Err(AppError::bad_request(
            "Return line does not belong to this invoice",
        ));
    }
    let old_credit: Decimal = invoice.get("credit_total");
    let old_paid: Decimal = invoice.get("paid");
    let new_credit = old_credit + credit_total;
    let new_net = invoice.get::<Decimal, _>("total") - new_credit;
    if new_net < Decimal::ZERO {
        return Err(AppError::conflict("Return credit exceeds invoice amount"));
    }
    let excess = (old_paid - new_net).max(Decimal::ZERO);
    let refund = if mode == "immediate" {
        excess
    } else {
        Decimal::ZERO
    };
    let settlement_credit = excess - refund;
    if refund > Decimal::ZERO && !body.refund_confirmed {
        return Err(AppError::conflict(
            "Confirm the immediate customer cash refund before posting this return",
        ));
    }
    let cash_account =
        crate::treasury::cash_account(&mut tx, body.cash_account_code.as_deref()).await?;
    if refund > Decimal::ZERO {
        crate::treasury::require_balance(&mut tx, &cash_account, refund).await?;
    }
    let return_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let number:String=sqlx::query_scalar("SELECT 'SR-'||CASE WHEN length(n::text)<6 THEN lpad(n::text,6,'0') ELSE n::text END FROM nextval('sales_return_number') n").fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'sales_return',$3,$4)").bind(journal_id).bind(format!("Customer return {number}: {reason}")).bind(return_id).bind(user.id).execute(&mut *tx).await?;
    post_lines(
        &mut tx,
        journal_id,
        &[
            ("4000", credit_total, Decimal::ZERO),
            ("1200", Decimal::ZERO, credit_total - excess),
            (&cash_account, Decimal::ZERO, refund),
            ("1300", cost_total, Decimal::ZERO),
            ("5000", Decimal::ZERO, cost_total),
        ],
    )
    .await?;
    if settlement_credit > Decimal::ZERO {
        let control = crate::settlements::party_control_account(&mut tx, "customer").await?;
        post_lines(
            &mut tx,
            journal_id,
            &[(&control, Decimal::ZERO, settlement_credit)],
        )
        .await?;
    }
    sqlx::query("INSERT INTO sales_returns(id,number,invoice_id,reason,credit_total,refund,cost_total,journal_id,created_by,settlement_mode,settlement_credit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(return_id).bind(&number).bind(id).bind(&reason).bind(credit_total).bind(refund).bind(cost_total).bind(journal_id).bind(user.id).bind(mode).bind(settlement_credit).execute(&mut *tx).await?;
    if settlement_credit > Decimal::ZERO {
        let customer_id: Uuid = sqlx::query_scalar("SELECT customer_id FROM invoices WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let fund = crate::settlements::create_source_fund(
            &mut tx,
            &user,
            crate::settlements::SourceFund {
                side: "customer",
                party_id: customer_id,
                kind: "return_credit",
                amount: settlement_credit,
                source_type: "sales_return",
                source_id: return_id,
                journal_id,
                reference: Some(&number),
            },
        )
        .await?;
        if mode == "deferred" {
            crate::settlements::reserve_refund(&mut tx, &user, fund, settlement_credit, &reason)
                .await?;
        }
    }
    for line in lines {
        let return_line_id = Uuid::new_v4();
        let mut movement_id = None;
        let mut before_quantity = None;
        let mut before_value = None;
        let mut before_average = None;
        if line.kind == "stock" {
            if let Some(input) = body
                .items
                .iter()
                .find(|item| item.invoice_line_id == line.invoice_line_id)
            {
                crate::inventory_extensions::set_context(
                    &mut tx,
                    crate::inventory_extensions::StockContext {
                        warehouse_id: input.warehouse_id,
                        lot_id: input.lot_id,
                        allocations: input.allocations.clone(),
                    },
                )
                .await?;
            }
            let product=sqlx::query("SELECT kind,quantity,inventory_value,average_cost FROM products WHERE id=$1 FOR UPDATE").bind(line.product_id).fetch_one(&mut *tx).await?;
            if product.get::<String, _>("kind") != "stock" {
                return Err(AppError::conflict(
                    "The original stock product is no longer available for a return",
                ));
            }
            let quantity: Decimal = product.get("quantity");
            let value: Decimal = product.get("inventory_value");
            let new_quantity = quantity + line.quantity;
            let new_value = value + line.cost;
            if new_quantity > Decimal::from(1_000_000_000u64)
                || new_value > Decimal::from(1_000_000_000_000u64)
            {
                return Err(AppError::bad_request(
                    "Return would exceed inventory limits",
                ));
            }
            before_quantity = Some(quantity);
            before_value = Some(value);
            before_average = Some(product.get::<Decimal, _>("average_cost"));
            sqlx::query(
                "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
            )
            .bind(line.product_id)
            .bind(new_quantity)
            .bind(new_value)
            .bind(domain::average(new_value, new_quantity))
            .execute(&mut *tx)
            .await?;
            let movement = Uuid::new_v4();
            movement_id = Some(movement);
            sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,sales_return_id) VALUES($1,$2,'sales_return',$3,$4,$5,$6,$7,$8,$9)").bind(movement).bind(line.product_id).bind(line.quantity).bind(domain::average(line.cost,line.quantity)).bind(line.cost).bind(&number).bind(invoice.get::<Uuid,_>("order_id")).bind(user.id).bind(return_id).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO sales_return_lines(id,return_id,invoice_line_id,product_id,product_sku,product_name,product_kind,quantity,credit,cost,movement_id,before_quantity,before_value,before_average) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(return_line_id).bind(return_id).bind(line.invoice_line_id).bind(line.product_id).bind(line.sku).bind(line.name).bind(line.kind).bind(line.quantity).bind(line.credit).bind(line.cost).bind(movement_id).bind(before_quantity).bind(before_value).bind(before_average).execute(&mut *tx).await?;
        for source in line.sources {
            sqlx::query("INSERT INTO sales_return_sources(return_line_id,shipment_allocation_id,quantity,cost) VALUES($1,$2,$3,$4)").bind(return_line_id).bind(source.allocation_id).bind(source.quantity).bind(source.cost).execute(&mut *tx).await?;
        }
    }
    let new_paid = old_paid - excess;
    sqlx::query(
        "UPDATE invoices SET credit_total=$2,paid=$3,status=$4,version=version+1 WHERE id=$1",
    )
    .bind(id)
    .bind(new_credit)
    .bind(new_paid)
    .bind(invoice_status(new_net, new_paid))
    .execute(&mut *tx)
    .await?;
    audit(&mut tx,&user,"sales_return.posted","sales_return",return_id,json!({"invoice_id":id,"credit_total":credit_total.to_string(),"refund":refund.to_string(),"refund_mode":mode,"settlement_credit":settlement_credit.to_string(),"cost_total":cost_total.to_string(),"reason":reason})).await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
fn invoice_status(net: Decimal, paid: Decimal) -> &'static str {
    if paid == net {
        "paid"
    } else if paid == Decimal::ZERO {
        "unpaid"
    } else {
        "partial"
    }
}
async fn post_lines(
    tx: &mut Transaction<'_, Postgres>,
    journal_id: Uuid,
    lines: &[(&str, Decimal, Decimal)],
) -> AppResult<()> {
    for (account, debit, credit) in lines {
        if *debit == Decimal::ZERO && *credit == Decimal::ZERO {
            continue;
        }
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)").bind(Uuid::new_v4()).bind(journal_id).bind(account).bind(debit).bind(credit).execute(&mut **tx).await?;
    }
    Ok(())
}
async fn reverse_return(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = reason(&body.reason)?;
    let request = payload("sales_return.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let original=sqlx::query("SELECT invoice_id,credit_total,refund,settlement_credit,cost_total,journal_id FROM sales_returns WHERE id=$1").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    let invoice_id: Uuid = original.get("invoice_id");
    let invoice = lock_invoice(&mut tx, invoice_id, body.version).await?;
    if let Some(reason) =
        crate::settlements::source_fund_blocker(&mut tx, "sales_return", id).await?
    {
        return Err(AppError::conflict(reason));
    }
    crate::settlements::reverse_source_fund(&mut tx, &user, "sales_return", id, &reason).await?;
    let blocked: Option<String> = sqlx::query_scalar("SELECT sales_return_blocked_reason($1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(blocked) = blocked {
        return Err(AppError::conflict(blocked));
    }
    let lines=sqlx::query("SELECT r.*,m.movement_seq FROM sales_return_lines r LEFT JOIN inventory_movements m ON m.id=r.movement_id WHERE r.return_id=$1 ORDER BY r.product_id").bind(id).fetch_all(&mut *tx).await?;
    let reversal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'sales_return',$2,$3,$4)").bind(reversal_id).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    for line in lines {
        if line.get::<String, _>("product_kind") != "stock" {
            continue;
        }
        let product_id: Uuid = line.get("product_id");
        let product = sqlx::query(
            "SELECT quantity,inventory_value,average_cost FROM products WHERE id=$1 FOR UPDATE",
        )
        .bind(product_id)
        .fetch_one(&mut *tx)
        .await?;
        let later:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_movements WHERE product_id=$1 AND movement_seq>$2)").bind(product_id).bind(line.get::<i64,_>("movement_seq")).fetch_one(&mut *tx).await?;
        if later {
            return Err(AppError::conflict(
                "This return has later stock movements; correct those through their source documents",
            ));
        }
        let before_quantity: Decimal = line.get("before_quantity");
        let before_value: Decimal = line.get("before_value");
        let quantity: Decimal = line.get("quantity");
        let cost: Decimal = line.get("cost");
        if product.get::<Decimal, _>("quantity") != before_quantity + quantity
            || product.get::<Decimal, _>("inventory_value") != before_value + cost
            || product.get::<Decimal, _>("average_cost")
                != domain::average(before_value + cost, before_quantity + quantity)
        {
            return Err(AppError::conflict(
                "Stock balance no longer matches the return snapshot",
            ));
        }
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(before_quantity)
        .bind(before_value)
        .bind(line.get::<Decimal, _>("before_average"))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,reverses_id,sales_return_id) VALUES($1,$2,'sales_return_reversal',$3,$4,$5,$6,$7,$8,$9,$10)").bind(Uuid::new_v4()).bind(product_id).bind(-quantity).bind(domain::average(cost,quantity)).bind(cost).bind(&reason).bind(invoice.get::<Uuid,_>("order_id")).bind(user.id).bind(line.get::<Uuid,_>("movement_id")).bind(id).execute(&mut *tx).await?;
    }
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)").bind(journal_id).bind(format!("Reverse customer return: {reason}")).bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$1,account_code,credit,debit FROM journal_lines WHERE journal_id=$2").bind(journal_id).bind(original.get::<Uuid,_>("journal_id")).execute(&mut *tx).await?;
    let new_credit =
        invoice.get::<Decimal, _>("credit_total") - original.get::<Decimal, _>("credit_total");
    let new_paid = invoice.get::<Decimal, _>("paid")
        + original.get::<Decimal, _>("refund")
        + original.get::<Decimal, _>("settlement_credit");
    let net = invoice.get::<Decimal, _>("total") - new_credit;
    if new_credit < Decimal::ZERO || new_paid > net {
        return Err(AppError::conflict(
            "Invoice balance no longer permits this return reversal",
        ));
    }
    sqlx::query(
        "UPDATE invoices SET credit_total=$2,paid=$3,status=$4,version=version+1 WHERE id=$1",
    )
    .bind(invoice_id)
    .bind(new_credit)
    .bind(new_paid)
    .bind(invoice_status(net, new_paid))
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        &user,
        "sales_return.reversed",
        "sales_return",
        id,
        json!({"invoice_id":invoice_id,"reversal_id":reversal_id,"reason":reason}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(invoice_id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
