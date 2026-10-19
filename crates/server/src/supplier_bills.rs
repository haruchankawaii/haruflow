use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    corrections::check_open,
    error::{AppError, AppResult},
    purchasing::check_version,
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
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/purchase-bills", get(bills).post(create))
        .route("/purchase-bills/summary", get(summary))
        .route("/purchase-bills/{id}", get(detail))
        .route("/purchase-bills/{id}/reverse", post(reverse))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BillInput {
    receipt_id: Uuid,
    order_version: i32,
    supplier_invoice_number: String,
    invoice_date: String,
    due_date: String,
    notes: Option<String>,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

fn clean_text(value: &str, min: usize, max: usize, multiline: bool) -> AppResult<String> {
    // Check the original input before trimming or serializing it into PostgreSQL
    // JSONB, which cannot represent NUL. Boundary whitespace must not hide controls.
    if value
        .chars()
        .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(AppError::bad_request(
            "Teks mengandung karakter kontrol yang tidak didukung.",
        ));
    }
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count()) {
        return Err(AppError::bad_request(format!(
            "Teks harus berisi {min}–{max} karakter."
        )));
    }
    Ok(value.to_owned())
}

fn date(value: &str) -> AppResult<NaiveDate> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
        || &value[..4] == "0000"
    {
        return Err(AppError::bad_request(
            "Tanggal harus berupa YYYY-MM-DD antara 0001-01-01 dan 9999-12-31.",
        ));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Tanggal kalender tidak valid."))
}

const BILL_SELECT: &str = r#"
    SELECT b.id,b.created_at,jsonb_build_object(
        'id',b.id,'number',b.number,'supplier_id',b.supplier_id,'supplier_code',b.supplier_code,
        'supplier_name',b.supplier_name,'supplier_invoice_number',b.supplier_invoice_number,
        'receipt_id',b.receipt_id,'receipt_number',r.number,'order_id',b.order_id,'order_number',o.number,
        'invoice_date',b.invoice_date::text,'due_date',b.due_date::text,'total',b.total::text,
        'paid','0.00','outstanding',CASE WHEN b.status='void' THEN '0.00' ELSE b.total::text END,
        'status',b.status,'due_status',CASE WHEN b.status='void' THEN 'void'
            WHEN b.due_date<cfg.today THEN 'overdue' WHEN b.due_date=cfg.today THEN 'due_today' ELSE 'upcoming' END,
        'version',b.version,'notes',b.notes,'journal_id',b.journal_id,'created_at',b.created_at,
        'can_reverse',purchase_bill_blocked_reason(b.id) IS NULL,
        'reversal_blocked_reason',purchase_bill_blocked_reason(b.id),
        'reversal',CASE WHEN rv.id IS NULL THEN NULL ELSE jsonb_build_object(
            'id',rv.id,'reason',rv.reason,'created_at',rv.created_at,'journal_id',rj.id) END,
        'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
            'id',l.id,'movement_id',l.movement_id,'product_id',l.product_id,'product_sku',l.product_sku,
            'product_name',l.product_name,'unit',l.unit,'quantity',l.quantity::text,
            'unit_cost',l.unit_cost::text,'value',l.value::text) ORDER BY l.position)
            FROM purchase_receipt_lines l WHERE l.receipt_id=b.receipt_id),'[]'::jsonb)
    ) AS document
    FROM purchase_bills b JOIN purchase_receipts r ON r.id=b.receipt_id JOIN purchase_orders o ON o.id=b.order_id
    CROSS JOIN (SELECT (now() AT TIME ZONE timezone)::date today FROM company) cfg
    LEFT JOIN posting_reversals rv ON rv.source_type='purchase_bill' AND rv.source_id=b.id
    LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=rv.id
"#;

async fn read_bill(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Value> {
    sqlx::query_scalar(&format!(
        "SELECT document FROM ({BILL_SELECT} WHERE b.id=$1) bill"
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(AppError::not_found)
}

async fn bills(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(&format!(
        "SELECT COALESCE(jsonb_agg(document ORDER BY created_at DESC,id),'[]'::jsonb) FROM ({BILL_SELECT} ORDER BY b.created_at DESC,b.id LIMIT 500) bills"
    )).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

async fn detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(&format!(
        "SELECT document FROM ({BILL_SELECT} WHERE b.id=$1) bill"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(AppError::not_found)?;
    Ok(Json(result))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<BillInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let supplier_invoice_number = clean_text(&body.supplier_invoice_number, 1, 100, false)?;
    let invoice_date = date(&body.invoice_date)?;
    let due_date = date(&body.due_date)?;
    if due_date < invoice_date {
        return Err(AppError::bad_request(
            "Jatuh tempo tidak boleh sebelum tanggal invoice.",
        ));
    }
    let notes = match body.notes.as_deref() {
        Some(value) => {
            // Validate controls even when the trimmed note would be empty.
            clean_text(value, 0, 2000, true)?;
            if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().to_owned())
            }
        }
        None => None,
    };
    let request = payload("purchase_bill.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let today: NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date FROM company")
            .fetch_one(&mut *tx)
            .await?;
    if invoice_date > today {
        return Err(AppError::bad_request(
            "Tanggal invoice tidak boleh melewati hari ini pada zona waktu perusahaan.",
        ));
    }
    let receipt = sqlx::query("SELECT order_id,total,number FROM purchase_receipts WHERE id=$1")
        .bind(body.receipt_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let order_id: Uuid = receipt.get("order_id");
    // All bill and receipt writes acquire accounting -> PO -> bill resource locks.
    // The supplier snapshot of received goods remains valid after deactivation.
    let order = sqlx::query("SELECT version,supplier_id,supplier_code,supplier_name,number FROM purchase_orders WHERE id=$1 FOR UPDATE")
        .bind(order_id).fetch_one(&mut *tx).await?;
    check_version(body.order_version, order.get("version"))?;
    let blocked: Option<String> =
        sqlx::query_scalar("SELECT purchase_receipt_bill_blocked_reason($1)")
            .bind(body.receipt_id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(reason) = blocked {
        return Err(AppError::conflict(reason));
    }
    let total: Decimal = receipt.get("total");
    let id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let sequence: i64 = sqlx::query_scalar("SELECT nextval('purchase_bill_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("PB-{sequence:06}");
    let accrual: String = sqlx::query_scalar(
        "SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'",
    )
    .fetch_one(&mut *tx)
    .await?;
    let payable: String =
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'purchase_bill',$3,$4)")
        .bind(journal_id).bind(format!("Supplier bill {number} / {}", receipt.get::<String,_>("number")))
        .bind(id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,0),($5,$2,$6,0,$4)")
        .bind(Uuid::new_v4()).bind(journal_id).bind(accrual).bind(total).bind(Uuid::new_v4()).bind(payable)
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO purchase_bills(id,number,receipt_id,order_id,supplier_id,supplier_code,supplier_name,supplier_invoice_number,invoice_date,due_date,notes,total,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
        .bind(id).bind(&number).bind(body.receipt_id).bind(order_id).bind(order.get::<Uuid,_>("supplier_id"))
        .bind(order.get::<String,_>("supplier_code")).bind(order.get::<String,_>("supplier_name"))
        .bind(&supplier_invoice_number).bind(invoice_date).bind(due_date).bind(notes).bind(total).bind(journal_id).bind(user.id)
        .execute(&mut *tx).await?;
    sqlx::query("UPDATE purchase_orders SET updated_at=now(),version=version+1 WHERE id=$1")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    let result = read_bill(&mut tx, id).await?;
    audit(&mut tx,&user,"purchase_bill.created","purchase_bill",id,
        json!({"receipt_id":body.receipt_id,"order_id":order_id,"journal_id":journal_id,"after":result})).await?;
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
        "purchase_bill.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let order_id: Uuid = sqlx::query_scalar("SELECT order_id FROM purchase_bills WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    sqlx::query("SELECT id FROM purchase_orders WHERE id=$1 FOR UPDATE")
        .bind(order_id)
        .fetch_one(&mut *tx)
        .await?;
    let bill = sqlx::query(
        "SELECT version,status,number,journal_id FROM purchase_bills WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    check_version(body.version, bill.get("version"))?;
    if bill.get::<String, _>("status") != "unpaid" {
        return Err(AppError::conflict("Tagihan supplier sudah dibalik."));
    }
    let reversal_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'purchase_bill',$2,$3,$4)")
        .bind(reversal_id).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal_id).bind(format!("Reversal of supplier bill {}",bill.get::<String,_>("number")))
        .bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    let lines = sqlx::query(
        "SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY id",
    )
    .bind(bill.get::<Uuid, _>("journal_id"))
    .fetch_all(&mut *tx)
    .await?;
    for line in lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(journal_id).bind(line.get::<String,_>("account_code"))
            .bind(line.get::<Decimal,_>("credit")).bind(line.get::<Decimal,_>("debit")).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE purchase_bills SET status='void',version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE purchase_orders SET updated_at=now(),version=version+1 WHERE id=$1")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    let result = read_bill(&mut tx, id).await?;
    audit(&mut tx,&user,"purchase_bill.reversed","purchase_bill",id,
        json!({"order_id":order_id,"reversal_id":reversal_id,"journal_id":journal_id,"reason":reason,"after":result})).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn summary(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(r#"
        WITH cfg AS (SELECT timezone,(now() AT TIME ZONE timezone)::date today FROM company),
        active AS (SELECT b.*,cfg.today FROM purchase_bills b CROSS JOIN cfg WHERE b.status='unpaid'),
        supplier_totals AS (
            SELECT a.supplier_id,s.code supplier_code,s.name supplier_name,count(*) bill_count,
                sum(a.total) outstanding_total,COALESCE(sum(a.total) FILTER (WHERE a.due_date<a.today),0) overdue_total
            FROM active a JOIN suppliers s ON s.id=a.supplier_id GROUP BY a.supplier_id,s.code,s.name
        )
        SELECT jsonb_build_object('today',cfg.today::text,'timezone',cfg.timezone,'as_of',now(),
            'counts',jsonb_build_object('unpaid',(SELECT count(*) FROM active),
                'void',(SELECT count(*) FROM purchase_bills WHERE status='void'),
                'overdue',(SELECT count(*) FROM active WHERE due_date<today),
                'due_today',(SELECT count(*) FROM active WHERE due_date=today),
                'upcoming',(SELECT count(*) FROM active WHERE due_date>today)),
            'outstanding_total',COALESCE((SELECT sum(total) FROM active),0)::text,
            'overdue_total',COALESCE((SELECT sum(total) FROM active WHERE due_date<today),0)::text,
            'due_today_total',COALESCE((SELECT sum(total) FROM active WHERE due_date=today),0)::text,
            'upcoming_total',COALESCE((SELECT sum(total) FROM active WHERE due_date>today),0)::text,
            'suppliers',COALESCE((SELECT jsonb_agg(jsonb_build_object('supplier_id',supplier_id,
                'supplier_code',supplier_code,'supplier_name',supplier_name,'bill_count',bill_count,
                'outstanding_total',outstanding_total::text,'overdue_total',overdue_total::text)
                ORDER BY outstanding_total DESC,supplier_name,supplier_id) FROM supplier_totals),'[]'::jsonb))
        FROM cfg
    "#).fetch_one(&state.db).await?;
    Ok(Json(result))
}
