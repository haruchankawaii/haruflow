use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, parse, payload, remember},
    corrections::check_open,
    error::{AppError, AppResult},
    purchasing::check_version,
    supplier_bills::{clean_text, read_bill},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/purchase-bills/analysis", get(analysis))
        .route("/purchase-bills/{id}/payments", post(pay))
        .route("/supplier-payments/{id}/reverse", post(reverse))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PaymentInput {
    version: i32,
    amount: String,
    reference: Option<String>,
    idempotency_key: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AnalysisFilter {
    months: Option<u8>,
}

async fn analysis(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<AnalysisFilter>,
) -> AppResult<Json<Value>> {
    let months = filter.months.unwrap_or(6);
    if ![3, 6, 12].contains(&months) {
        return Err(AppError::bad_request("Pilih rentang 3, 6, atau 12 bulan."));
    }
    let result:Value=sqlx::query_scalar(r#"
        WITH cfg AS (SELECT timezone,(now() AT TIME ZONE timezone)::date today,
            date_trunc('month',now() AT TIME ZONE timezone) current_month FROM company),
        outstanding AS (SELECT b.*,b.total-b.paid remaining,c.today-b.due_date days_overdue,
            CASE WHEN b.due_date>=c.today THEN 'current' WHEN c.today-b.due_date<=30 THEN 'days_1_30'
            WHEN c.today-b.due_date<=60 THEN 'days_31_60' WHEN c.today-b.due_date<=90 THEN 'days_61_90' ELSE 'over_90' END bucket
            FROM purchase_bills b CROSS JOIN cfg c WHERE b.status IN ('unpaid','partial')),
        buckets AS (SELECT key,position FROM (VALUES ('current',0),('days_1_30',1),('days_31_60',2),('days_61_90',3),('over_90',4)) x(key,position)),
        months AS (SELECT generate_series(current_month-($1::int-1)*interval '1 month',current_month,interval '1 month') starts FROM cfg),
        cash AS (SELECT date_trunc('month',j.created_at AT TIME ZONE c.timezone) starts,
            COALESCE(sum(l.credit),0) paid,COALESCE(sum(l.debit),0) reversed
            FROM journal_entries j JOIN journal_lines l ON l.journal_id=j.id AND l.account_code='1100'
            LEFT JOIN posting_reversals r ON j.source_type='reversal' AND r.id=j.source_id CROSS JOIN cfg c
            WHERE (j.source_type='supplier_payment' OR r.source_type='supplier_payment')
              AND j.created_at>=(c.current_month-($1::int-1)*interval '1 month') AT TIME ZONE c.timezone AND j.created_at<=now() GROUP BY 1)
        SELECT jsonb_build_object('today',cfg.today::text,'timezone',cfg.timezone,'as_of',now(),'months',$1::int,
            'aging', (SELECT jsonb_agg(jsonb_build_object('key',k.key,'count',(SELECT count(*) FROM outstanding WHERE bucket=k.key),
                'total',COALESCE((SELECT sum(remaining) FROM outstanding WHERE bucket=k.key),0)::text) ORDER BY position) FROM buckets k),
            'bills',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',id,'number',number,'supplier_id',supplier_id,'supplier_name',supplier_name,
                'supplier_invoice_number',supplier_invoice_number,'due_date',due_date::text,'days_overdue',greatest(days_overdue,0),
                'bucket',bucket,'total',total::text,'paid',paid::text,'outstanding',remaining::text) ORDER BY due_date,id) FROM outstanding),'[]'::jsonb),
            'series',(SELECT jsonb_agg(jsonb_build_object('month',to_char(m.starts,'YYYY-MM'),'paid',COALESCE(c.paid,0)::text,
                'reversed',COALESCE(c.reversed,0)::text,'net',(COALESCE(c.paid,0)-COALESCE(c.reversed,0))::text) ORDER BY m.starts)
                FROM months m LEFT JOIN cash c ON c.starts=m.starts),
            'cash_balance',(SELECT COALESCE(sum(debit-credit),0)::text FROM journal_lines WHERE account_code='1100')) FROM cfg
    "#).bind(i32::from(months)).fetch_one(&state.db).await?;
    Ok(Json(result))
}

async fn pay(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PaymentInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let amount = parse(&body.amount, 2, "Amount")?;
    if amount <= Decimal::ZERO {
        return Err(AppError::bad_request(
            "Pembayaran harus lebih besar dari nol.",
        ));
    }
    let reference = body
        .reference
        .as_deref()
        .map(|v| clean_text(v, 0, 200, false))
        .transpose()?
        .filter(|v| !v.is_empty());
    let request = payload(
        "supplier_payment.create",
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
        "SELECT status,total,paid,version,number FROM purchase_bills WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    check_version(body.version, bill.get("version"))?;
    let paid: Decimal = bill.get("paid");
    let total: Decimal = bill.get("total");
    if bill.get::<String, _>("status") == "void" || amount > total - paid {
        return Err(AppError::conflict(
            "Pembayaran melampaui sisa utang atau tagihan sudah dibatalkan.",
        ));
    }
    let cash: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(&mut *tx)
    .await?;
    if amount > cash {
        return Err(AppError::conflict(
            "Saldo kas tidak cukup untuk pembayaran ini.",
        ));
    }
    let payable: String =
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
            .fetch_one(&mut *tx)
            .await?;
    let payment_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let sequence: i64 = sqlx::query_scalar("SELECT nextval('supplier_payment_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("SP-{sequence:06}");
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'supplier_payment',$3,$4)")
        .bind(journal_id).bind(format!("Pembayaran {number} / {}",bill.get::<String,_>("number"))).bind(payment_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,0),($5,$2,'1100',0,$4)")
        .bind(Uuid::new_v4()).bind(journal_id).bind(payable).bind(amount).bind(Uuid::new_v4()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO supplier_payments(id,number,bill_id,amount,reference,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(payment_id).bind(number).bind(id).bind(amount).bind(reference).bind(journal_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE purchase_bills SET paid=paid+$2,status=CASE WHEN paid+$2=total THEN 'paid' ELSE 'partial' END,version=version+1 WHERE id=$1")
        .bind(id).bind(amount).execute(&mut *tx).await?;
    let result = read_bill(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "supplier_payment.created",
        "supplier_payment",
        payment_id,
        json!({"bill_id":id,"amount":amount.to_string(),"journal_id":journal_id}),
    )
    .await?;
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
        "supplier_payment.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let payment=sqlx::query("SELECT p.bill_id,p.amount,p.journal_id,p.number,b.order_id FROM supplier_payments p JOIN purchase_bills b ON b.id=p.bill_id WHERE p.id=$1")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    sqlx::query("SELECT id FROM purchase_orders WHERE id=$1 FOR UPDATE")
        .bind(payment.get::<Uuid, _>("order_id"))
        .fetch_one(&mut *tx)
        .await?;
    let bill_id: Uuid = payment.get("bill_id");
    let bill = sqlx::query("SELECT version,status,paid FROM purchase_bills WHERE id=$1 FOR UPDATE")
        .bind(bill_id)
        .fetch_one(&mut *tx)
        .await?;
    check_version(body.version, bill.get("version"))?;
    let reversed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='supplier_payment' AND source_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    if reversed || bill.get::<String, _>("status") == "void" {
        return Err(AppError::conflict(
            "Pembayaran sudah dibalik atau tagihan tidak aktif.",
        ));
    }
    let amount: Decimal = payment.get("amount");
    if bill.get::<Decimal, _>("paid") < amount {
        return Err(AppError::conflict("Saldo pembayaran tidak cocok."));
    }
    let reversal_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'supplier_payment',$2,$3,$4)").bind(reversal_id).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)").bind(journal_id).bind(format!("Pembalikan {}",payment.get::<String,_>("number"))).bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$2,account_code,credit,debit FROM journal_lines WHERE journal_id=$1")
        .bind(payment.get::<Uuid,_>("journal_id")).bind(journal_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE purchase_bills SET paid=paid-$2,status=CASE WHEN paid-$2=0 THEN 'unpaid' ELSE 'partial' END,version=version+1 WHERE id=$1").bind(bill_id).bind(amount).execute(&mut *tx).await?;
    let result = read_bill(&mut tx, bill_id).await?;
    audit(
        &mut tx,
        &user,
        "supplier_payment.reversed",
        "supplier_payment",
        id,
        json!({"bill_id":bill_id,"reason":reason,"journal_id":journal_id}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
