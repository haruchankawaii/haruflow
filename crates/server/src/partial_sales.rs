use crate::{
    AppState,
    auth::AuthUser,
    business::{
        INVOICE_SELECT, ORDER_SELECT, audit, cached, journal, parse, payload, remember, text,
    },
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
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/orders/{id}/shipments/{shipment_id}/reverse",
        post(reverse_one_shipment),
    )
}

#[derive(Deserialize, Serialize)]
pub(crate) struct AllocationInput {
    line_id: Uuid,
    quantity: String,
}
#[derive(Deserialize, Serialize)]
pub(crate) struct PartialInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Vec<AllocationInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<i64>,
    idempotency_key: Uuid,
}

fn quantities(items: &Option<Vec<AllocationInput>>) -> AppResult<Option<BTreeMap<Uuid, Decimal>>> {
    items
        .as_ref()
        .map(|items| {
            if items.is_empty() || items.len() > 100 {
                return Err(AppError::bad_request("Choose 1–100 order lines"));
            }
            let mut values = BTreeMap::new();
            for item in items {
                let quantity = parse(&item.quantity, 3, "Quantity")?;
                if values.insert(item.line_id, quantity).is_some() {
                    return Err(AppError::bad_request("Combine duplicate order lines"));
                }
            }
            Ok(values)
        })
        .transpose()
}

async fn lock_order(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: Option<i64>,
) -> AppResult<sqlx::postgres::PgRow> {
    let order = sqlx::query(
        "SELECT status,number,customer_id,version FROM sales_orders WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(AppError::not_found)?;
    if ["draft", "cancelled"].contains(&order.get::<String, _>("status").as_str()) {
        return Err(AppError::conflict("Confirm the order before fulfillment"));
    }
    if version.is_some_and(|v| v != order.get::<i64, _>("version")) {
        return Err(AppError::conflict(
            "Order changed; reload before continuing",
        ));
    }
    Ok(order)
}

pub(crate) async fn ship_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PartialInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let selected = quantities(&body.items)?;
    let request = payload("order.ship", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let order = lock_order(&mut tx, id, body.version).await?;
    let lines=sqlx::query("SELECT l.id,l.product_id,l.quantity,p.shipped FROM sales_order_lines l JOIN sales_line_progress($1) p ON p.line_id=l.id WHERE l.order_id=$1 AND l.product_kind='stock' ORDER BY l.product_id")
        .bind(id).fetch_all(&mut *tx).await?;
    let mut pending = selected.clone().unwrap_or_default();
    let mut chosen = Vec::new();
    for line in lines {
        let line_id: Uuid = line.get("id");
        let remaining = line.get::<Decimal, _>("quantity") - line.get::<Decimal, _>("shipped");
        let quantity = if selected.is_some() {
            pending.remove(&line_id).unwrap_or(Decimal::ZERO)
        } else {
            remaining
        };
        if quantity > remaining {
            return Err(AppError::conflict(
                "Shipment exceeds undelivered order quantity",
            ));
        }
        if quantity > Decimal::ZERO {
            chosen.push((line_id, line.get::<Uuid, _>("product_id"), quantity));
        }
    }
    if !pending.is_empty() {
        return Err(AppError::bad_request(
            "Shipment line is not a stock item on this order",
        ));
    }
    if chosen.is_empty() {
        return Err(AppError::conflict("No stock quantity remains to ship"));
    }
    let posting_id = Uuid::new_v4();
    sqlx::query("INSERT INTO shipment_postings(id,order_id) VALUES($1,$2)")
        .bind(posting_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let mut total = Decimal::ZERO;
    for (line_id, product_id, quantity) in chosen {
        let product=sqlx::query("SELECT sku,kind,quantity,average_cost,inventory_value FROM products WHERE id=$1 FOR UPDATE")
            .bind(product_id).fetch_one(&mut *tx).await?;
        let before_quantity: Decimal = product.get("quantity");
        let before_value: Decimal = product.get("inventory_value");
        let before_average: Decimal = product.get("average_cost");
        if product.get::<String, _>("kind") != "stock" || before_quantity < quantity {
            return Err(AppError::conflict(format!(
                "Insufficient stock for {}",
                product.get::<String, _>("sku")
            )));
        }
        let cost = domain::shipment_cost(quantity, before_quantity, before_average, before_value);
        let remaining = before_quantity - quantity;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(remaining)
        .bind(before_value - cost)
        .bind(if remaining == Decimal::ZERO {
            Decimal::ZERO
        } else {
            before_average
        })
        .execute(&mut *tx)
        .await?;
        let movement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,shipment_posting_id) VALUES($1,$2,'shipment',$3,$4,$5,$6,$7,$8,$9)")
            .bind(movement_id).bind(product_id).bind(-quantity).bind(before_average).bind(cost).bind(order.get::<String,_>("number")).bind(id).bind(user.id).bind(posting_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO shipment_allocations(id,shipment_id,line_id,quantity,movement_id,before_quantity,before_value,before_average) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(Uuid::new_v4()).bind(posting_id).bind(line_id).bind(quantity).bind(movement_id).bind(before_quantity).bind(before_value).bind(before_average).execute(&mut *tx).await?;
        total += cost;
    }
    journal(
        &mut tx,
        &user,
        "shipment",
        posting_id,
        &format!("Shipment {}", order.get::<String, _>("number")),
        ("5000", "1300"),
        total,
    )
    .await?;
    refresh(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "order.shipped",
        "sales_order",
        id,
        json!({"shipment_id":posting_id,"cost":total.to_string()}),
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

pub(crate) async fn invoice_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PartialInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let selected = quantities(&body.items)?;
    let request = payload("order.invoice", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let order = lock_order(&mut tx, id, body.version).await?;
    // Legacy full requests retain the requirement to deliver mixed orders first.
    let not_delivered:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales_order_lines l JOIN sales_line_progress($1) p ON p.line_id=l.id WHERE l.order_id=$1 AND l.product_kind='stock') AND NOT EXISTS(SELECT 1 FROM sales_line_progress($1) WHERE shipped>0)")
        .bind(id).fetch_one(&mut *tx).await?;
    if selected.is_none() && not_delivered {
        return Err(AppError::conflict(
            "Ship stock before invoicing, or choose fulfilled/service lines explicitly",
        ));
    }
    let lines=sqlx::query("SELECT l.id,l.quantity,l.total,l.product_kind,p.shipped,p.invoiced,p.invoiced_value FROM sales_order_lines l JOIN sales_line_progress($1) p ON p.line_id=l.id WHERE l.order_id=$1 ORDER BY l.product_id")
        .bind(id).fetch_all(&mut *tx).await?;
    let mut pending = selected.clone().unwrap_or_default();
    let mut chosen = Vec::new();
    let mut total = Decimal::ZERO;
    for line in lines {
        let line_id: Uuid = line.get("id");
        let ordered: Decimal = line.get("quantity");
        let invoiced: Decimal = line.get("invoiced");
        let service = line.get::<String, _>("product_kind") == "service";
        let remaining = (if service {
            ordered
        } else {
            line.get("shipped")
        }) - invoiced;
        let quantity = if selected.is_some() {
            pending.remove(&line_id).unwrap_or(Decimal::ZERO)
        } else {
            remaining
        };
        if quantity > remaining {
            return Err(AppError::conflict(
                "Invoice exceeds fulfilled, uninvoiced quantity",
            ));
        }
        if quantity == Decimal::ZERO {
            continue;
        }
        // Cumulative allocation makes the final invoice exactly equal the agreed
        // rounded order-line total, including sub-cent intermediate quantities.
        let mut value =
            domain::money(line.get::<Decimal, _>("total") * (invoiced + quantity) / ordered)
                - line.get::<Decimal, _>("invoiced_value");
        if invoiced + quantity < ordered {
            // Leave a cent for the final quantity. Otherwise a tiny remainder
            // could become impossible to invoice after an early round-up.
            value = value.min(
                (line.get::<Decimal, _>("total")
                    - line.get::<Decimal, _>("invoiced_value")
                    - Decimal::new(1, 2))
                .max(Decimal::ZERO),
            );
        }
        if value <= Decimal::ZERO {
            return Err(AppError::conflict(
                "Selected quantity is too small to invoice separately; combine it with the remaining quantity",
            ));
        }
        total += value;
        chosen.push((line_id, quantity, value, service));
    }
    if !pending.is_empty() {
        return Err(AppError::bad_request(
            "Invoice line does not belong to this order",
        ));
    }
    if chosen.is_empty() || total <= Decimal::ZERO {
        return Err(AppError::conflict(
            "Selected invoice must contain a positive billable amount",
        ));
    }
    let invoice_id = Uuid::new_v4();
    let number:String=sqlx::query_scalar("SELECT invoice_prefix||'-'||CASE WHEN length(n::text)<6 THEN lpad(n::text,6,'0') ELSE n::text END FROM company CROSS JOIN nextval('invoice_number') n").fetch_one(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO invoices(id,number,order_id,customer_id,total) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(invoice_id)
    .bind(&number)
    .bind(id)
    .bind(order.get::<Uuid, _>("customer_id"))
    .bind(total)
    .execute(&mut *tx)
    .await?;
    for (line_id, quantity, value, service) in chosen {
        let allocation_id = Uuid::new_v4();
        sqlx::query("INSERT INTO invoice_allocations(id,invoice_id,line_id,quantity,total) VALUES($1,$2,$3,$4,$5)").bind(allocation_id).bind(invoice_id).bind(line_id).bind(quantity).bind(value).execute(&mut *tx).await?;
        if service {
            continue;
        }
        let deliveries=sqlx::query("SELECT a.id,a.quantity-COALESCE((SELECT sum(x.quantity) FROM invoice_shipment_allocations x JOIN invoice_allocations ia ON ia.id=x.invoice_allocation_id JOIN invoices i ON i.id=ia.invoice_id WHERE x.shipment_allocation_id=a.id AND i.status<>'void'),0) available FROM shipment_allocations a JOIN shipment_postings s ON s.id=a.shipment_id WHERE a.line_id=$1 AND NOT s.reversed ORDER BY s.created_at,s.id")
            .bind(line_id).fetch_all(&mut *tx).await?;
        let mut remainder = quantity;
        for delivery in deliveries {
            let available: Decimal = delivery.get("available");
            let used = available.min(remainder);
            if used > Decimal::ZERO {
                sqlx::query("INSERT INTO invoice_shipment_allocations(invoice_allocation_id,shipment_allocation_id,quantity) VALUES($1,$2,$3)").bind(allocation_id).bind(delivery.get::<Uuid,_>("id")).bind(used).execute(&mut *tx).await?;
                remainder -= used;
            }
            if remainder == Decimal::ZERO {
                break;
            }
        }
        if remainder != Decimal::ZERO {
            return Err(AppError::conflict(
                "Delivery allocations changed; reload the order",
            ));
        }
    }
    journal(
        &mut tx,
        &user,
        "invoice",
        invoice_id,
        &format!("Invoice {number}"),
        ("1200", "4000"),
        total,
    )
    .await?;
    refresh(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "invoice.posted",
        "invoice",
        invoice_id,
        json!({"order_id":id,"total":total.to_string()}),
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

pub(crate) async fn refresh(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<()> {
    sqlx::query("SELECT refresh_sales_order($1)")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ReverseInput {
    reason: String,
    idempotency_key: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<i64>,
}
pub(crate) async fn reverse_latest_shipment(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    reverse_shipment(state, user, id, None, body).await
}
async fn reverse_one_shipment(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, shipment_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    reverse_shipment(state, user, id, Some(shipment_id), body).await
}
async fn reverse_shipment(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    shipment_id: Option<Uuid>,
    body: ReverseInput,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let reason = text(&body.reason, 1000, "Reason")?;
    if reason.chars().count() < 3 || reason.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "Reason must contain 3–1000 characters without control characters",
        ));
    }
    let request_body = match shipment_id {
        Some(shipment_id) => json!({"id":id,"shipment_id":shipment_id,"body":body}),
        None => json!({"id":id,"body":body}),
    };
    let request = payload("shipment.reverse", &user, &request_body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    lock_order(&mut tx, id, body.version).await?;
    let posting_id:Uuid=sqlx::query_scalar("SELECT id FROM shipment_postings WHERE order_id=$1 AND NOT reversed AND ($2::uuid IS NULL OR id=$2) ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE")
        .bind(id).bind(shipment_id).fetch_optional(&mut *tx).await?.ok_or_else(||AppError::conflict("No active shipment found"))?;
    let billed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM shipment_allocations a JOIN invoice_shipment_allocations x ON x.shipment_allocation_id=a.id JOIN invoice_allocations ia ON ia.id=x.invoice_allocation_id JOIN invoices i ON i.id=ia.invoice_id WHERE a.shipment_id=$1 AND i.status<>'void')").bind(posting_id).fetch_one(&mut *tx).await?;
    if billed {
        return Err(AppError::conflict(
            "Reverse the invoices allocated to this shipment first",
        ));
    }
    let movements=sqlx::query("SELECT m.id,m.product_id,m.quantity,m.value,m.unit_cost,m.movement_seq,a.before_quantity,a.before_value,a.before_average FROM shipment_allocations a JOIN inventory_movements m ON m.id=a.movement_id WHERE a.shipment_id=$1 ORDER BY m.product_id")
        .bind(posting_id).fetch_all(&mut *tx).await?;
    if movements.is_empty() {
        return Err(AppError::conflict("Shipment has no stock movements"));
    }
    let reversal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'shipment',$2,$3,$4)").bind(reversal_id).bind(posting_id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
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
        let later:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_movements WHERE product_id=$1 AND movement_seq>$2)").bind(product_id).bind(movement.get::<i64,_>("movement_seq")).fetch_one(&mut *tx).await?;
        let average = if !later
            && movement.get::<Option<Decimal>, _>("before_quantity") == Some(new_quantity)
            && movement.get::<Option<Decimal>, _>("before_value") == Some(new_value)
        {
            movement
                .get::<Option<Decimal>, _>("before_average")
                .unwrap_or(domain::average(new_value, new_quantity))
        } else {
            domain::average(new_value, new_quantity)
        };
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(new_quantity)
        .bind(new_value)
        .bind(average)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,reverses_id,shipment_posting_id) VALUES($1,$2,'shipment_reversal',$3,$4,$5,$6,$7,$8,$9,$10)").bind(Uuid::new_v4()).bind(product_id).bind(quantity).bind(movement.get::<Decimal,_>("unit_cost")).bind(value).bind(&reason).bind(id).bind(user.id).bind(movement.get::<Uuid,_>("id")).bind(posting_id).execute(&mut *tx).await?;
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
    refresh(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "shipment.reversed",
        "sales_order",
        id,
        json!({"shipment_id":posting_id,"reversal_id":reversal_id,"reason":reason}),
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
