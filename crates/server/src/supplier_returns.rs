use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
    purchasing::check_version,
    supplier_bills::{clean_text, read_bill},
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
        .route("/purchase-bills/{id}/returns", post(create))
        .route("/supplier-returns/{id}/reverse", post(reverse))
        .route("/supplier-returns/{id}", get(detail))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReturnInput {
    version: i32,
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
struct ReturnItem {
    bill_line_id: Uuid,
    quantity: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

async fn lock_bill(
    tx: &mut Transaction<'_, Postgres>,
    bill_id: Uuid,
    version: i32,
) -> AppResult<sqlx::postgres::PgRow> {
    let order_id: Uuid = sqlx::query_scalar("SELECT order_id FROM purchase_bills WHERE id=$1")
        .bind(bill_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    sqlx::query("SELECT id FROM purchase_orders WHERE id=$1 FOR UPDATE")
        .bind(order_id)
        .fetch_one(&mut **tx)
        .await?;
    let bill=sqlx::query("SELECT id,order_id,number,status,total,paid,credit_total,version FROM purchase_bills WHERE id=$1 FOR UPDATE").bind(bill_id).fetch_one(&mut **tx).await?;
    check_version(version, bill.get("version"))?;
    if bill.get::<String, _>("status") == "void" {
        return Err(AppError::conflict("Tagihan supplier sudah dibatalkan."));
    }
    Ok(bill)
}
async fn account(tx: &mut Transaction<'_, Postgres>, role: &str) -> AppResult<String> {
    Ok(
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role=$1")
            .bind(role)
            .fetch_one(&mut **tx)
            .await?,
    )
}
async fn journal_line(
    tx: &mut Transaction<'_, Postgres>,
    journal: Uuid,
    account: &str,
    signed: Decimal,
) -> AppResult<()> {
    if signed == Decimal::ZERO {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(journal)
    .bind(account)
    .bind(signed.max(Decimal::ZERO))
    .bind((-signed).max(Decimal::ZERO))
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReturnInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mode = crate::sales_returns::settlement_mode(body.refund_mode.as_deref())?;
    let reason = clean_text(&body.reason, 3, 1000, true)?;
    if body.items.is_empty() || body.items.len() > 100 {
        return Err(AppError::bad_request("Pilih 1-100 barang retur."));
    }
    let mut selected = BTreeMap::new();
    for item in &body.items {
        let quantity =
            domain::decimal(&item.quantity, 3, "Jumlah retur").map_err(AppError::bad_request)?;
        if selected.insert(item.bill_line_id, quantity).is_some() {
            return Err(AppError::bad_request("Gabungkan barang retur yang sama."));
        }
    }
    let request = payload(
        "supplier_return.create",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let bill = lock_bill(&mut tx, id, body.version).await?;
    sqlx::query("SELECT id FROM products WHERE id IN(SELECT product_id FROM purchase_bill_lines WHERE bill_id=$1) ORDER BY id FOR UPDATE").bind(id).fetch_all(&mut *tx).await?;
    let lines=sqlx::query("SELECT l.*,supplier_bill_returned_quantity(l.id) returned_quantity,supplier_bill_returned_credit(l.id) returned_credit,p.quantity stock_quantity,p.average_cost stock_average,p.inventory_value stock_value,p.unit stock_unit,p.kind FROM purchase_bill_lines l JOIN products p ON p.id=l.product_id WHERE l.bill_id=$1 ORDER BY l.position").bind(id).fetch_all(&mut *tx).await?;
    let mut allocations = Vec::new();
    let mut total = Decimal::ZERO;
    let mut carrying = Decimal::ZERO;
    for line in lines {
        let Some(quantity) = selected.remove(&line.get::<Uuid, _>("id")) else {
            continue;
        };
        let original_quantity: Decimal = line.get("quantity");
        let returned_quantity: Decimal = line.get("returned_quantity");
        let remaining = original_quantity - returned_quantity;
        if quantity > remaining {
            return Err(AppError::conflict(
                "Jumlah retur melebihi sisa barang pada tagihan.",
            ));
        }
        if line.get::<String, _>("kind") != "stock"
            || line.get::<String, _>("stock_unit") != line.get::<String, _>("unit")
        {
            return Err(AppError::conflict(
                "Satuan atau jenis katalog berbeda dari barang pada tagihan. Periksa sebelum retur.",
            ));
        }
        let on_hand: Decimal = line.get("stock_quantity");
        if quantity > on_hand {
            return Err(AppError::conflict(
                "Stok saat ini tidak cukup untuk retur supplier.",
            ));
        }
        let returned_credit: Decimal = line.get("returned_credit");
        let original_credit: Decimal = line.get("value");
        let remaining_credit = original_credit - returned_credit;
        let credit = if quantity == remaining {
            remaining_credit
        } else {
            (domain::money(original_credit * (returned_quantity + quantity) / original_quantity)
                - returned_credit)
                .max(Decimal::ZERO)
                .min((remaining_credit - Decimal::new(1, 2)).max(Decimal::ZERO))
        };
        if credit <= Decimal::ZERO {
            return Err(AppError::bad_request(
                "Nilai setiap barang retur harus lebih dari nol setelah pembulatan; perbesar jumlah.",
            ));
        }
        let value = domain::shipment_cost(
            quantity,
            on_hand,
            line.get("stock_average"),
            line.get("stock_value"),
        );
        total += credit;
        carrying += value;
        allocations.push((line, quantity, credit, value));
    }
    if !selected.is_empty() || allocations.is_empty() {
        return Err(AppError::bad_request(
            "Barang retur tidak berasal dari tagihan ini.",
        ));
    }
    let previous_paid: Decimal = bill.get("paid");
    let previous_credit: Decimal = bill.get("credit_total");
    let net = bill.get::<Decimal, _>("total") - previous_credit - total;
    if net < Decimal::ZERO {
        return Err(AppError::conflict(
            "Nilai retur melebihi sisa nilai tagihan.",
        ));
    }
    let excess = (previous_paid - net).max(Decimal::ZERO);
    let refund = if mode == "immediate" {
        excess
    } else {
        Decimal::ZERO
    };
    let settlement_credit = excess - refund;
    if refund > Decimal::ZERO && !body.refund_confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi bahwa pengembalian dana supplier benar-benar sudah diterima sebelum mencatat retur ini.",
        ));
    }
    let return_id = Uuid::new_v4();
    let journal = Uuid::new_v4();
    let seq: i64 = sqlx::query_scalar("SELECT nextval('supplier_return_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("PR-{seq:06}");
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'supplier_return',$3,$4)").bind(journal).bind(format!("Retur {number} / {}",bill.get::<String,_>("number"))).bind(return_id).bind(user.id).execute(&mut *tx).await?;
    let payable = account(&mut tx, "purchase_payable").await?;
    let variance = account(&mut tx, "purchase_return_variance").await?;
    journal_line(&mut tx, journal, &payable, total - excess).await?;
    journal_line(&mut tx, journal, "1100", refund).await?;
    if settlement_credit > Decimal::ZERO {
        let control = crate::settlements::party_control_account(&mut tx, "supplier").await?;
        journal_line(&mut tx, journal, &control, settlement_credit).await?;
    }
    journal_line(&mut tx, journal, "1300", -carrying).await?;
    journal_line(&mut tx, journal, &variance, carrying - total).await?;
    sqlx::query("INSERT INTO supplier_returns(id,number,bill_id,credit_total,inventory_value,refund,reason,journal_id,created_by,settlement_mode,settlement_credit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
 .bind(return_id).bind(&number).bind(id).bind(total).bind(carrying).bind(refund).bind(&reason).bind(journal).bind(user.id).bind(mode).bind(settlement_credit).execute(&mut *tx).await?;
    if settlement_credit > Decimal::ZERO {
        let supplier_id: Uuid =
            sqlx::query_scalar("SELECT supplier_id FROM purchase_bills WHERE id=$1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let fund = crate::settlements::create_source_fund(
            &mut tx,
            &user,
            crate::settlements::SourceFund {
                side: "supplier",
                party_id: supplier_id,
                kind: "return_credit",
                amount: settlement_credit,
                source_type: "supplier_return",
                source_id: return_id,
                journal_id: journal,
                reference: Some(&number),
            },
        )
        .await?;
        if mode == "deferred" {
            crate::settlements::reserve_refund(&mut tx, &user, fund, settlement_credit, &reason)
                .await?;
        }
    }
    for (position, (line, quantity, credit, value)) in allocations.iter().enumerate() {
        let product_id: Uuid = line.get("product_id");
        let before: Decimal = line.get("stock_quantity");
        let before_value: Decimal = line.get("stock_value");
        let before_average: Decimal = line.get("stock_average");
        let remaining = before - quantity;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(remaining)
        .bind(before_value - value)
        .bind(if remaining == Decimal::ZERO {
            Decimal::ZERO
        } else {
            before_average
        })
        .execute(&mut *tx)
        .await?;
        let movement = Uuid::new_v4();
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,supplier_return_id) VALUES($1,$2,'supplier_return',$3,$4,$5,$6,$7,$8)").bind(movement).bind(product_id).bind(-quantity).bind(before_average).bind(value).bind(&number).bind(user.id).bind(return_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO supplier_return_lines(id,return_id,bill_line_id,movement_id,position,product_id,product_sku,product_name,unit,quantity,credit,value,unit_cost,before_quantity,before_value,before_average) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
  .bind(Uuid::new_v4()).bind(return_id).bind(line.get::<Uuid,_>("id")).bind(movement).bind((position+1) as i32).bind(product_id).bind(line.get::<String,_>("product_sku")).bind(line.get::<String,_>("product_name")).bind(line.get::<String,_>("unit"))
  .bind(quantity).bind(credit).bind(value).bind(before_average).bind(before).bind(before_value).bind(before_average).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE purchase_bills SET credit_total=credit_total+$2,paid=paid-$3,status=CASE WHEN paid-$3=total-credit_total-$2 THEN 'paid' WHEN paid-$3=0 THEN 'unpaid' ELSE 'partial' END,version=version+1 WHERE id=$1").bind(id).bind(total).bind(excess).execute(&mut *tx).await?;
    sqlx::query("UPDATE purchase_orders SET version=version+1,updated_at=now() WHERE id=$1")
        .bind(bill.get::<Uuid, _>("order_id"))
        .execute(&mut *tx)
        .await?;
    let result = read_bill(&mut tx, id).await?;
    audit(&mut tx,&user,"supplier_return.created","supplier_return",return_id,json!({"bill_id":id,"credit":total.to_string(),"inventory_value":carrying.to_string(),"refund":refund.to_string(),"refund_mode":mode,"settlement_credit":settlement_credit.to_string(),"reason":reason,"journal_id":journal})).await?;
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
    user.require(&["finance"])?;
    let reason = clean_text(&body.reason, 3, 1000, true)?;
    let request = payload(
        "supplier_return.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let source = sqlx::query("SELECT * FROM supplier_returns WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let bill_id: Uuid = source.get("bill_id");
    let bill = lock_bill(&mut tx, bill_id, body.version).await?;
    if let Some(reason) =
        crate::settlements::source_fund_blocker(&mut tx, "supplier_return", id).await?
    {
        return Err(AppError::conflict(reason));
    }
    crate::settlements::reverse_source_fund(&mut tx, &user, "supplier_return", id, &reason).await?;
    sqlx::query("SELECT id FROM products WHERE id IN(SELECT product_id FROM supplier_return_lines WHERE return_id=$1) ORDER BY id FOR UPDATE").bind(id).fetch_all(&mut *tx).await?;
    let blocked: Option<String> = sqlx::query_scalar("SELECT supplier_return_blocked_reason($1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(reason) = blocked {
        return Err(AppError::conflict(reason));
    }
    let lines =
        sqlx::query("SELECT * FROM supplier_return_lines WHERE return_id=$1 ORDER BY position")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    for line in &lines {
        if line.get::<Decimal, _>("before_quantity") > Decimal::from(1_000_000_000u64)
            || line.get::<Decimal, _>("before_value") > Decimal::from(1_000_000_000_000u64)
        {
            return Err(AppError::conflict(
                "Saldo pemulihan retur melebihi batas stok.",
            ));
        }
    }
    let reversal = Uuid::new_v4();
    let journal = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'supplier_return',$2,$3,$4)").bind(reversal).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)").bind(journal).bind(format!("Pembalikan retur {}",source.get::<String,_>("number"))).bind(reversal).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$2,account_code,credit,debit FROM journal_lines WHERE journal_id=$1").bind(source.get::<Uuid,_>("journal_id")).bind(journal).execute(&mut *tx).await?;
    for line in lines {
        let product_id: Uuid = line.get("product_id");
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(line.get::<Decimal, _>("before_quantity"))
        .bind(line.get::<Decimal, _>("before_value"))
        .bind(line.get::<Decimal, _>("before_average"))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,reverses_id,supplier_return_id) VALUES($1,$2,'supplier_return_reversal',$3,$4,$5,$6,$7,$8,$9)")
  .bind(Uuid::new_v4()).bind(product_id).bind(line.get::<Decimal,_>("quantity")).bind(line.get::<Decimal,_>("unit_cost")).bind(line.get::<Decimal,_>("value")).bind(&reason).bind(user.id).bind(line.get::<Uuid,_>("movement_id")).bind(id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE purchase_bills SET credit_total=credit_total-$2,paid=paid+$3,status=CASE WHEN paid+$3=total-credit_total+$2 THEN 'paid' WHEN paid+$3=0 THEN 'unpaid' ELSE 'partial' END,version=version+1 WHERE id=$1")
 .bind(bill_id).bind(source.get::<Decimal,_>("credit_total")).bind(source.get::<Decimal,_>("refund")+source.get::<Decimal,_>("settlement_credit")).execute(&mut *tx).await?;
    sqlx::query("UPDATE purchase_orders SET version=version+1,updated_at=now() WHERE id=$1")
        .bind(bill.get::<Uuid, _>("order_id"))
        .execute(&mut *tx)
        .await?;
    let result = read_bill(&mut tx, bill_id).await?;
    audit(
        &mut tx,
        &user,
        "supplier_return.reversed",
        "supplier_return",
        id,
        json!({"bill_id":bill_id,"reason":reason,"journal_id":journal}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result:Option<Value>=sqlx::query_scalar("SELECT item FROM supplier_returns r CROSS JOIN LATERAL jsonb_array_elements(supplier_return_history(r.bill_id)) item WHERE r.id=$1 AND item->>'id'=r.id::text").bind(id).fetch_optional(&state.db).await?;
    Ok(Json(result.ok_or_else(AppError::not_found)?))
}
