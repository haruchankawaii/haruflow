use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, journal, payload, remember, text},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/party-funds", get(list_funds).post(create_fund))
        .route("/party-funds/{id}", get(fund_detail))
        .route("/party-funds/{id}/allocate", post(allocate))
        .route("/party-funds/{id}/reverse", post(reverse_fund))
        .route("/party-allocations/{id}/reverse", post(reverse_allocation))
        .route("/party-funds/{id}/refund-requests", post(request_refund))
        .route("/party-refund-requests/{id}/cancel", post(cancel_request))
        .route("/party-refund-requests/{id}/pay", post(pay_request))
        .route("/party-funds/{id}/refunds", post(direct_refund))
        .route("/party-refunds/{id}/reverse", post(reverse_refund))
        .route("/party-targets", get(targets))
        .route("/parties/{side}/{id}/statement", get(statement))
        .route(
            "/customers/{id}/terms",
            axum::routing::patch(customer_terms),
        )
        .route("/invoices/{id}/terms", axum::routing::patch(update_terms))
}

fn side(value: &str) -> AppResult<()> {
    if matches!(value, "customer" | "supplier") {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "Jenis pihak harus pelanggan atau supplier",
        ))
    }
}
fn amount(value: &str) -> AppResult<Decimal> {
    domain::payment_amount(value).map_err(AppError::bad_request)
}
fn reason(value: &str) -> AppResult<String> {
    let v = text(value, 1000, "Alasan")?;
    if v.chars().count() < 3 || v.chars().any(char::is_control) {
        Err(AppError::bad_request(
            "Alasan harus berisi 3–1.000 karakter yang dapat ditampilkan",
        ))
    } else {
        Ok(v)
    }
}
fn optional(value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| {
            let s = text(v, max, "Referensi")?;
            if s.chars().any(char::is_control) {
                Err(AppError::bad_request(
                    "Referensi tidak boleh berisi karakter kontrol",
                ))
            } else {
                Ok(s)
            }
        })
        .transpose()
}
fn date(value: &str) -> AppResult<NaiveDate> {
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Tanggal harus memakai format YYYY-MM-DD"))?;
    if !(1..=9999).contains(&d.year()) || d.to_string() != value {
        return Err(AppError::bad_request(
            "Tanggal harus memakai format YYYY-MM-DD",
        ));
    }
    Ok(d)
}
async fn party_name(db: &mut PgConnection, kind: &str, id: Uuid) -> AppResult<String> {
    side(kind)?;
    let sql = if kind == "customer" {
        "SELECT name FROM customers WHERE id=$1"
    } else {
        "SELECT name FROM suppliers WHERE id=$1"
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(AppError::not_found)
}
pub(crate) async fn party_control_account(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
) -> AppResult<String> {
    side(kind)?;
    Ok(
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role=$1")
            .bind(if kind == "customer" {
                "customer_credit"
            } else {
                "supplier_advance"
            })
            .fetch_one(&mut **tx)
            .await?,
    )
}
async fn obligation_account(tx: &mut Transaction<'_, Postgres>, kind: &str) -> AppResult<String> {
    if kind == "customer" {
        Ok("1200".into())
    } else {
        Ok(
            sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
                .fetch_one(&mut **tx)
                .await?,
        )
    }
}
async fn cash_sufficient(tx: &mut Transaction<'_, Postgres>, value: Decimal) -> AppResult<()> {
    let cash: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(&mut **tx)
    .await?;
    if cash < value {
        Err(AppError::conflict("Saldo kas tidak mencukupi"))
    } else {
        Ok(())
    }
}
async fn read_fund(db: &mut PgConnection, id: Uuid) -> AppResult<Value> {
    sqlx::query_scalar::<_, Option<Value>>("SELECT party_fund_document($1)")
        .bind(id)
        .fetch_one(db)
        .await?
        .ok_or_else(AppError::not_found)
}
async fn locked_fund(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    version: i64,
) -> AppResult<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM party_funds WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    if row.get::<i64, _>("version") != version {
        return Err(AppError::conflict(
            "Saldo dana berubah; muat ulang sebelum melanjutkan",
        ));
    }
    let reversed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='party_fund' AND source_id=$1)").bind(id).fetch_one(&mut **tx).await?;
    if reversed {
        return Err(AppError::conflict("Sumber dana sudah dibalik"));
    }
    Ok(row)
}
async fn touch(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE party_funds SET version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn available(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Decimal> {
    Ok(
        sqlx::query_scalar("SELECT available FROM party_fund_balances($1)")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?,
    )
}
async fn reversal(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    kind: &str,
    id: Uuid,
    note: &str,
) -> AppResult<Uuid> {
    let key = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,$2,$3,$4,$5)").bind(key).bind(kind).bind(id).bind(note).bind(user.id).execute(&mut **tx).await?;
    Ok(key)
}

pub(crate) struct SourceFund<'a> {
    pub side: &'a str,
    pub party_id: Uuid,
    pub kind: &'a str,
    pub amount: Decimal,
    pub source_type: &'a str,
    pub source_id: Uuid,
    pub journal_id: Uuid,
    pub reference: Option<&'a str>,
}
pub(crate) async fn create_source_fund(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    input: SourceFund<'_>,
) -> AppResult<Uuid> {
    insert_source_fund(tx, user, input, None, None).await
}
async fn insert_source_fund(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    input: SourceFund<'_>,
    description: Option<&str>,
    order_id: Option<Uuid>,
) -> AppResult<Uuid> {
    side(input.side)?;
    party_name(tx, input.side, input.party_id).await?;
    amount(&input.amount.to_string())?;
    let id = if input.source_type == "party_fund" {
        input.source_id
    } else {
        Uuid::new_v4()
    };
    let number: String = sqlx::query_scalar("SELECT 'PF-'||nextval('party_fund_number')::text")
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO party_funds(id,number,side,party_id,customer_id,supplier_id,kind,amount,source_type,source_id,journal_id,created_by,reference,description,order_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
 .bind(id).bind(number).bind(input.side).bind(input.party_id).bind(if input.side=="customer"{Some(input.party_id)}else{None}).bind(if input.side=="supplier"{Some(input.party_id)}else{None})
 .bind(input.kind).bind(input.amount).bind(input.source_type).bind(input.source_id).bind(input.journal_id).bind(user.id).bind(input.reference).bind(description).bind(order_id).execute(&mut **tx).await?;
    audit(tx,user,"party_fund.source_recorded","party_fund",id,json!({"source_type":input.source_type,"source_id":input.source_id,"amount":input.amount.to_string()})).await?;
    Ok(id)
}
pub(crate) async fn source_fund_blocker(
    tx: &mut Transaction<'_, Postgres>,
    source_type: &str,
    source_id: Uuid,
) -> AppResult<Option<String>> {
    let row=sqlx::query("SELECT f.id,b.* FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b WHERE source_type=$1 AND source_id=$2 FOR UPDATE OF f").bind(source_type).bind(source_id).fetch_optional(&mut **tx).await?;
    Ok(row.and_then(|r|if r.get::<Decimal,_>("allocated")>Decimal::ZERO||r.get::<Decimal,_>("refunded")>Decimal::ZERO||r.get::<Decimal,_>("reserved")>Decimal::ZERO{Some("Balikkan alokasi dan refund, lalu batalkan permintaan refund tertunda sebelum membalik sumber ini".into())}else{None}))
}
pub(crate) async fn reverse_source_fund(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    source_type: &str,
    source_id: Uuid,
    note: &str,
) -> AppResult<()> {
    if let Some(message) = source_fund_blocker(tx, source_type, source_id).await? {
        return Err(AppError::conflict(message));
    }
    let id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM party_funds f WHERE source_type=$1 AND source_id=$2 AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_fund' AND r.source_id=f.id)").bind(source_type).bind(source_id).fetch_optional(&mut **tx).await?;
    if let Some(id) = id {
        reversal(tx, user, "party_fund", id, note).await?;
        touch(tx, id).await?;
    }
    Ok(())
}
pub(crate) async fn reserve_refund(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    fund_id: Uuid,
    value: Decimal,
    note: &str,
) -> AppResult<Uuid> {
    if value <= Decimal::ZERO || value > available(tx, fund_id).await? {
        return Err(AppError::conflict(
            "Dana refund yang disisihkan melebihi saldo tersedia",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO party_refund_requests(id,fund_id,amount,reason,created_by) VALUES($1,$2,$3,$4,$5)").bind(id).bind(fund_id).bind(value).bind(note).bind(user.id).execute(&mut **tx).await?;
    touch(tx, fund_id).await?;
    Ok(id)
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PartyFilter {
    side: Option<String>,
    party_id: Option<Uuid>,
}
async fn list_funds(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PartyFilter>,
) -> AppResult<Json<Value>> {
    if let Some(s) = &filter.side {
        side(s)?;
    }
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(party_fund_document(id) ORDER BY created_at DESC,id DESC),'[]'::jsonb) FROM party_funds WHERE ($1::text IS NULL OR side=$1) AND ($2::uuid IS NULL OR party_id=$2)").bind(filter.side).bind(filter.party_id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
async fn fund_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    Ok(Json(read_fund(&mut *state.db.acquire().await?, id).await?))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FundInput {
    side: String,
    party_id: Uuid,
    amount: String,
    reference: Option<String>,
    description: Option<String>,
    order_id: Option<Uuid>,
    idempotency_key: Uuid,
}
async fn create_fund(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<FundInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    side(&body.side)?;
    let value = amount(&body.amount)?;
    let reference = optional(body.reference.as_deref(), 200)?;
    let description = optional(body.description.as_deref(), 1000)?;
    let request = payload("party_fund.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    party_name(&mut tx, &body.side, body.party_id).await?;
    if let Some(order) = body.order_id {
        let query = if body.side == "customer" {
            "SELECT EXISTS(SELECT 1 FROM sales_orders WHERE id=$1 AND customer_id=$2)"
        } else {
            "SELECT EXISTS(SELECT 1 FROM purchase_orders WHERE id=$1 AND supplier_id=$2)"
        };
        let valid: bool = sqlx::query_scalar(query)
            .bind(order)
            .bind(body.party_id)
            .fetch_one(&mut *tx)
            .await?;
        if !valid {
            return Err(AppError::bad_request(
                "Pesanan bukan milik pihak yang dipilih",
            ));
        }
    }
    if body.side == "supplier" {
        cash_sufficient(&mut tx, value).await?;
    }
    let id = Uuid::new_v4();
    let control = party_control_account(&mut tx, &body.side).await?;
    let accounts = if body.side == "customer" {
        ("1100", control.as_str())
    } else {
        (control.as_str(), "1100")
    };
    journal(
        &mut tx,
        &user,
        "party_fund",
        id,
        "Uang muka pelanggan/supplier",
        accounts,
        value,
    )
    .await?;
    let journal_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM journal_entries WHERE source_type='party_fund' AND source_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    insert_source_fund(
        &mut tx,
        &user,
        SourceFund {
            side: &body.side,
            party_id: body.party_id,
            kind: "cash_advance",
            amount: value,
            source_type: "party_fund",
            source_id: id,
            journal_id,
            reference: reference.as_deref(),
        },
        description.as_deref(),
        body.order_id,
    )
    .await?;
    audit(&mut tx,&user,"party_fund.created","party_fund",id,json!({"side":body.side,"party_id":body.party_id,"amount":value.to_string(),"description":description,"order_id":body.order_id})).await?;
    let result = read_fund(&mut tx, id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VersionReason {
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}
async fn reverse_fund(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VersionReason>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let note = reason(&body.reason)?;
    let request = payload("party_fund.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let fund = locked_fund(&mut tx, id, body.version).await?;
    if fund.get::<String, _>("kind") != "cash_advance" {
        return Err(AppError::conflict(
            "Balikkan kredit ini melalui dokumen saldo awal atau retur sumbernya",
        ));
    }
    if let Some(blocker) = source_fund_blocker(&mut tx, "party_fund", id).await? {
        return Err(AppError::conflict(blocker));
    }
    let value: Decimal = fund.get("amount");
    let kind: String = fund.get("side");
    if kind == "customer" {
        cash_sufficient(&mut tx, value).await?;
    }
    let key = reversal(&mut tx, &user, "party_fund", id, &note).await?;
    let control = party_control_account(&mut tx, &kind).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        key,
        &format!("Pembalikan uang muka: {note}"),
        if kind == "customer" {
            (control.as_str(), "1100")
        } else {
            ("1100", control.as_str())
        },
        value,
    )
    .await?;
    touch(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "party_fund.reversed",
        "party_fund",
        id,
        json!({"reason":note}),
    )
    .await?;
    let result = read_fund(&mut tx, id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

struct Target {
    number: String,
    total: Decimal,
    paid: Decimal,
    party_id: Uuid,
    version: i64,
}
async fn lock_target(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    id: Uuid,
) -> AppResult<Target> {
    let row=match kind{
 "invoice"=>sqlx::query("SELECT number,total-credit_total total,paid,customer_id party_id,version FROM invoices WHERE id=$1 AND status<>'void' FOR UPDATE").bind(id).fetch_optional(&mut **tx).await?,
 "bill"=>sqlx::query("SELECT number,total-credit_total total,paid,supplier_id party_id,version::bigint version FROM purchase_bills WHERE id=$1 AND status<>'void' FOR UPDATE").bind(id).fetch_optional(&mut **tx).await?,
 "opening_receivable"|"opening_payable"=>{
 sqlx::query("INSERT INTO party_target_versions(target_type,target_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(kind).bind(id).execute(&mut **tx).await?;
 sqlx::query("SELECT version FROM party_target_versions WHERE target_type=$1 AND target_id=$2 FOR UPDATE").bind(kind).bind(id).execute(&mut **tx).await?;
 sqlx::query("SELECT o.reference number,o.amount total,COALESCE((SELECT sum(a.amount) FROM party_allocations a WHERE a.target_id=o.id AND a.target_type=$2 AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_allocation' AND r.source_id=a.id)),0) paid,o.party_id,v.version FROM opening_party_items o JOIN party_target_versions v ON v.target_type=$2 AND v.target_id=o.id WHERE o.id=$1 AND o.kind=$3 AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='opening' AND r.source_id=o.posting_id)")
 .bind(id).bind(kind).bind(if kind=="opening_receivable"{"receivable"}else{"payable"}).fetch_optional(&mut **tx).await?},
 _=>return Err(AppError::bad_request("Jenis tagihan tujuan tidak valid"))}.ok_or_else(AppError::not_found)?;
    Ok(Target {
        number: row.get("number"),
        total: row.get("total"),
        paid: row.get("paid"),
        party_id: row.get("party_id"),
        version: row.get("version"),
    })
}
async fn adjust_target(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    id: Uuid,
    value: Decimal,
) -> AppResult<()> {
    match kind {
        "invoice" => {
            sqlx::query("UPDATE invoices SET paid=paid+$2,status=CASE WHEN paid+$2=total-credit_total THEN 'paid' WHEN paid+$2=0 THEN 'unpaid' ELSE 'partial' END,version=version+1 WHERE id=$1").bind(id).bind(value).execute(&mut **tx).await?;
        }
        "bill" => {
            sqlx::query("UPDATE purchase_bills SET paid=paid+$2,status=CASE WHEN paid+$2=total-credit_total THEN 'paid' WHEN paid+$2=0 THEN 'unpaid' ELSE 'partial' END,version=version+1 WHERE id=$1").bind(id).bind(value).execute(&mut **tx).await?;
        }
        _ => {
            sqlx::query("UPDATE party_target_versions SET version=version+1 WHERE target_type=$1 AND target_id=$2").bind(kind).bind(id).execute(&mut **tx).await?;
        }
    }
    Ok(())
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AllocationInput {
    target_type: String,
    target_id: Uuid,
    amount: String,
    version: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AllocateInput {
    version: i64,
    items: Vec<AllocationInput>,
    idempotency_key: Uuid,
}
async fn allocate(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AllocateInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("party_fund.allocate", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let fund = locked_fund(&mut tx, id, body.version).await?;
    if body.items.is_empty() || body.items.len() > 100 {
        return Err(AppError::bad_request("Pilih 1–100 tagihan tujuan"));
    }
    let mut seen = BTreeSet::new();
    let mut total = Decimal::ZERO;
    let kind: String = fund.get("side");
    let party: Uuid = fund.get("party_id");
    let mut selected = Vec::new();
    for item in &body.items {
        if !seen.insert((&item.target_type, item.target_id)) {
            return Err(AppError::bad_request(
                "Gabungkan alokasi untuk tagihan tujuan yang sama",
            ));
        }
        if (kind == "customer"
            && !matches!(item.target_type.as_str(), "invoice" | "opening_receivable"))
            || (kind == "supplier"
                && !matches!(item.target_type.as_str(), "bill" | "opening_payable"))
        {
            return Err(AppError::bad_request(
                "Tagihan tujuan tidak sesuai dengan jenis pihak sumber dana",
            ));
        }
        let value = amount(&item.amount)?;
        total += value;
        selected.push((item, value));
    }
    if total > available(&mut tx, id).await? {
        return Err(AppError::conflict(
            "Alokasi melebihi saldo tersedia yang belum disisihkan untuk refund",
        ));
    }
    selected.sort_by_key(|(item, _)| (&item.target_type, item.target_id));
    let control = party_control_account(&mut tx, &kind).await?;
    let obligation = obligation_account(&mut tx, &kind).await?;
    for (item, value) in selected {
        let target = lock_target(&mut tx, &item.target_type, item.target_id).await?;
        if target.party_id != party {
            return Err(AppError::bad_request(
                "Dana tidak dapat dialokasikan kepada pihak lain",
            ));
        }
        if target.version != item.version {
            return Err(AppError::conflict(
                "Tagihan tujuan berubah; muat ulang sebelum mengalokasikan dana",
            ));
        }
        if value > target.total - target.paid {
            return Err(AppError::conflict("Alokasi melebihi sisa tagihan tujuan"));
        }
        let allocation_id = Uuid::new_v4();
        journal(
            &mut tx,
            &user,
            "party_allocation",
            allocation_id,
            &format!("Alokasi dana ke {}", target.number),
            if kind == "customer" {
                (control.as_str(), obligation.as_str())
            } else {
                (obligation.as_str(), control.as_str())
            },
            value,
        )
        .await?;
        let journal_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM journal_entries WHERE source_type='party_allocation' AND source_id=$1",
        )
        .bind(allocation_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO party_allocations(id,fund_id,target_type,target_id,target_number,amount,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(allocation_id).bind(id).bind(&item.target_type).bind(item.target_id).bind(&target.number).bind(value).bind(journal_id).bind(user.id).execute(&mut *tx).await?;
        adjust_target(&mut tx, &item.target_type, item.target_id, value).await?;
    }
    touch(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "party_fund.allocated",
        "party_fund",
        id,
        json!({"amount":total.to_string(),"targets":body.items.len()}),
    )
    .await?;
    let result = read_fund(&mut tx, id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn reverse_allocation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VersionReason>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let note = reason(&body.reason)?;
    let request = payload(
        "party_allocation.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let allocation = sqlx::query("SELECT * FROM party_allocations WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let fund_id: Uuid = allocation.get("fund_id");
    let fund = locked_fund(&mut tx, fund_id, body.version).await?;
    let done:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='party_allocation' AND source_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    if done {
        return Err(AppError::conflict("Alokasi sudah dibalik"));
    }
    let target_type: String = allocation.get("target_type");
    let target_id: Uuid = allocation.get("target_id");
    let target = lock_target(&mut tx, &target_type, target_id).await?;
    let value: Decimal = allocation.get("amount");
    if target.paid < value {
        return Err(AppError::conflict(
            "Balikkan retur atau penyelesaian berikutnya yang terkait terlebih dahulu",
        ));
    }
    let key = reversal(&mut tx, &user, "party_allocation", id, &note).await?;
    let kind: String = fund.get("side");
    let control = party_control_account(&mut tx, &kind).await?;
    let obligation = obligation_account(&mut tx, &kind).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        key,
        &format!("Pembalikan alokasi: {note}"),
        if kind == "customer" {
            (obligation.as_str(), control.as_str())
        } else {
            (control.as_str(), obligation.as_str())
        },
        value,
    )
    .await?;
    adjust_target(&mut tx, &target_type, target_id, -value).await?;
    touch(&mut tx, fund_id).await?;
    audit(
        &mut tx,
        &user,
        "party_allocation.reversed",
        "party_allocation",
        id,
        json!({"reason":note}),
    )
    .await?;
    let result = read_fund(&mut tx, fund_id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReserveInput {
    version: i64,
    amount: String,
    reason: String,
    idempotency_key: Uuid,
}
async fn request_refund(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReserveInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let value = amount(&body.amount)?;
    let note = reason(&body.reason)?;
    let request = payload("party_refund.request", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    locked_fund(&mut tx, id, body.version).await?;
    let request_id = reserve_refund(&mut tx, &user, id, value, &note).await?;
    audit(
        &mut tx,
        &user,
        "party_refund.requested",
        "party_refund_request",
        request_id,
        json!({"amount":value.to_string(),"fund_id":id}),
    )
    .await?;
    let result = read_fund(&mut tx, id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn cancel_request(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VersionReason>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let note = reason(&body.reason)?;
    let request = payload("party_refund.cancel", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let fund_id: Uuid = sqlx::query_scalar("SELECT fund_id FROM party_refund_requests WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    locked_fund(&mut tx, fund_id, body.version).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM party_refund_cancellations WHERE request_id=$1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(AppError::conflict("Permintaan refund sudah dibatalkan"));
    }
    sqlx::query("INSERT INTO party_refund_cancellations(id,request_id,reason,created_by) VALUES($1,$2,$3,$4)").bind(Uuid::new_v4()).bind(id).bind(&note).bind(user.id).execute(&mut *tx).await?;
    touch(&mut tx, fund_id).await?;
    audit(
        &mut tx,
        &user,
        "party_refund.cancelled",
        "party_refund_request",
        id,
        json!({"reason":note}),
    )
    .await?;
    let result = read_fund(&mut tx, fund_id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RefundInput {
    version: i64,
    amount: String,
    reference: Option<String>,
    reason: String,
    confirmed: bool,
    idempotency_key: Uuid,
}
async fn direct_refund(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RefundInput>,
) -> AppResult<Json<Value>> {
    post_refund(state, user, id, None, body).await
}
async fn pay_request(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RefundInput>,
) -> AppResult<Json<Value>> {
    let fund: Uuid = sqlx::query_scalar("SELECT fund_id FROM party_refund_requests WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(AppError::not_found)?;
    post_refund(state, user, fund, Some(id), body).await
}
async fn post_refund(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    request_id: Option<Uuid>,
    body: RefundInput,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let value = amount(&body.amount)?;
    let note = reason(&body.reason)?;
    let reference = optional(body.reference.as_deref(), 200)?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasikan bahwa dana benar-benar sudah berpindah",
        ));
    }
    let request = payload(
        "party_refund.pay",
        &user,
        &json!({"id":id,"request_id":request_id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let fund = locked_fund(&mut tx, id, body.version).await?;
    let budget = if let Some(key) = request_id {
        sqlx::query_scalar::<_,Decimal>("SELECT q.amount-COALESCE((SELECT sum(p.amount) FROM party_refunds p WHERE p.request_id=q.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_refund' AND r.source_id=p.id)),0) FROM party_refund_requests q WHERE q.id=$1 AND q.fund_id=$2 AND NOT EXISTS(SELECT 1 FROM party_refund_cancellations c WHERE c.request_id=q.id)").bind(key).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(||AppError::conflict("Permintaan refund dibatalkan atau tidak tersedia"))?
    } else {
        available(&mut tx, id).await?
    };
    if value > budget {
        return Err(AppError::conflict(
            "Refund melebihi saldo tersedia atau dana yang disisihkan",
        ));
    }
    let kind: String = fund.get("side");
    if kind == "customer" {
        cash_sufficient(&mut tx, value).await?;
    }
    let refund_id = Uuid::new_v4();
    let control = party_control_account(&mut tx, &kind).await?;
    journal(
        &mut tx,
        &user,
        "party_refund",
        refund_id,
        &format!("Refund pelanggan/supplier: {note}"),
        if kind == "customer" {
            (control.as_str(), "1100")
        } else {
            ("1100", control.as_str())
        },
        value,
    )
    .await?;
    let journal_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM journal_entries WHERE source_type='party_refund' AND source_id=$1",
    )
    .bind(refund_id)
    .fetch_one(&mut *tx)
    .await?;
    let number: String = sqlx::query_scalar("SELECT 'RF-'||nextval('party_refund_number')::text")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO party_refunds(id,number,fund_id,request_id,amount,reference,reason,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(refund_id).bind(number).bind(id).bind(request_id).bind(value).bind(reference).bind(&note).bind(journal_id).bind(user.id).execute(&mut *tx).await?;
    touch(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "party_refund.paid",
        "party_refund",
        refund_id,
        json!({"amount":value.to_string(),"fund_id":id}),
    )
    .await?;
    let result = read_fund(&mut tx, id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn reverse_refund(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VersionReason>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let note = reason(&body.reason)?;
    let request = payload("party_refund.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let refund = sqlx::query("SELECT fund_id,amount FROM party_refunds WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let fund_id: Uuid = refund.get("fund_id");
    let fund = locked_fund(&mut tx, fund_id, body.version).await?;
    let done:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='party_refund' AND source_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    if done {
        return Err(AppError::conflict("Refund sudah dibalik"));
    }
    let kind: String = fund.get("side");
    let value: Decimal = refund.get("amount");
    if kind == "supplier" {
        cash_sufficient(&mut tx, value).await?;
    }
    let key = reversal(&mut tx, &user, "party_refund", id, &note).await?;
    let control = party_control_account(&mut tx, &kind).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        key,
        &format!("Pembalikan refund pelanggan/supplier: {note}"),
        if kind == "customer" {
            ("1100", control.as_str())
        } else {
            (control.as_str(), "1100")
        },
        value,
    )
    .await?;
    touch(&mut tx, fund_id).await?;
    audit(
        &mut tx,
        &user,
        "party_refund.reversed",
        "party_refund",
        id,
        json!({"reason":note}),
    )
    .await?;
    let result = read_fund(&mut tx, fund_id).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

// Read models below include immutable legacy events; no old cash is posted again.
fn events_sql(kind: &str) -> String {
    let (
        docs,
        party,
        payments,
        payment_kind,
        returns,
        return_kind,
        document_kind,
        target,
        cash_sign,
        opening_kind,
    ) = if kind == "customer" {
        (
            "invoices",
            "customer_id",
            "payments",
            "payment",
            "sales_returns",
            "sales_return",
            "invoice",
            "invoice",
            1,
            "receivable",
        )
    } else {
        (
            "purchase_bills",
            "supplier_id",
            "supplier_payments",
            "supplier_payment",
            "supplier_returns",
            "supplier_return",
            "purchase_bill",
            "bill",
            -1,
            "payable",
        )
    };
    format!(
        r#"WITH events AS(
 SELECT d.id,d.{party} party_id,'{document_kind}'::text event_type,d.number,'{document_kind}'::text source_type,d.id source_id,'{target}'::text target_type,d.id target_id,j.id journal_id,d.created_at,d.total debt_delta,0::numeric credit_delta,0::numeric cash_delta,0::numeric document_credit,0::numeric settled_delta FROM {docs} d LEFT JOIN journal_entries j ON j.source_type='{document_kind}' AND j.source_id=d.id
 UNION ALL SELECT r.id,d.{party},'document_reversal',d.number,'reversal',r.id,'{target}',d.id,j.id,r.created_at,-d.total,0,0,0,0 FROM posting_reversals r JOIN {docs} d ON d.id=r.source_id AND r.source_type='{document_kind}' LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id
 UNION ALL SELECT p.id,d.{party},'payment',d.number,'{payment_kind}',p.id,'{target}',d.id,j.id,p.created_at,-p.amount,0,{cash_sign}*p.amount,0,p.amount FROM {payments} p JOIN {docs} d ON d.id=p.{payment_fk} LEFT JOIN journal_entries j ON j.source_type='{payment_kind}' AND j.source_id=p.id
 UNION ALL SELECT r.id,d.{party},'payment_reversal',d.number,'reversal',r.id,'{target}',d.id,j.id,r.created_at,p.amount,0,(-({cash_sign}))*p.amount,0,-p.amount FROM posting_reversals r JOIN {payments} p ON p.id=r.source_id AND r.source_type='{payment_kind}' JOIN {docs} d ON d.id=p.{payment_fk} LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id
 UNION ALL SELECT t.id,d.{party},'return',t.number,'{return_kind}',t.id,'{target}',d.id,t.journal_id,t.created_at,-(t.credit_total-t.refund-t.settlement_credit),0,(-({cash_sign}))*t.refund,t.credit_total,-(t.refund+t.settlement_credit) FROM {returns} t JOIN {docs} d ON d.id=t.{return_fk}
 UNION ALL SELECT r.id,d.{party},'return_reversal',t.number,'reversal',r.id,'{target}',d.id,j.id,r.created_at,t.credit_total-t.refund-t.settlement_credit,0,{cash_sign}*t.refund,-t.credit_total,t.refund+t.settlement_credit FROM posting_reversals r JOIN {returns} t ON t.id=r.source_id AND r.source_type='{return_kind}' JOIN {docs} d ON d.id=t.{return_fk} LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id
 UNION ALL SELECT o.id,o.party_id,'opening_{opening_kind}',o.reference,'opening_item',o.id,'opening_{opening_kind}',o.id,p.journal_id,o.created_at,o.amount,0,0,0,0 FROM opening_party_items o JOIN opening_postings p ON p.id=o.posting_id WHERE o.kind='{opening_kind}'
 UNION ALL SELECT md5(o.id::text||r.id::text)::uuid,o.party_id,'opening_reversal',o.reference,'reversal',r.id,'opening_{opening_kind}',o.id,j.id,r.created_at,-o.amount,0,0,0,0 FROM opening_party_items o JOIN posting_reversals r ON r.source_type='opening' AND r.source_id=o.posting_id LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE o.kind='{opening_kind}'
 UNION ALL SELECT f.id,f.party_id,CASE WHEN f.kind='cash_advance' THEN 'advance' ELSE f.kind END,f.number,f.source_type,f.source_id,NULL::text,NULL::uuid,f.journal_id,f.created_at,0,f.amount,CASE WHEN f.kind='cash_advance' THEN {cash_sign}*f.amount ELSE 0 END,0,0 FROM party_funds f WHERE f.side='{kind}'
 UNION ALL SELECT r.id,f.party_id,'fund_reversal',f.number,'reversal',r.id,NULL,NULL,j.id,r.created_at,0,-f.amount,CASE WHEN f.kind='cash_advance' THEN (-({cash_sign}))*f.amount ELSE 0 END,0,0 FROM posting_reversals r JOIN party_funds f ON f.id=r.source_id AND r.source_type='party_fund' LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE f.side='{kind}'
 UNION ALL SELECT a.id,f.party_id,'allocation',a.target_number,'party_allocation',a.id,a.target_type,a.target_id,a.journal_id,a.created_at,-a.amount,-a.amount,0,0,a.amount FROM party_allocations a JOIN party_funds f ON f.id=a.fund_id WHERE f.side='{kind}'
 UNION ALL SELECT r.id,f.party_id,'allocation_reversal',a.target_number,'reversal',r.id,a.target_type,a.target_id,j.id,r.created_at,a.amount,a.amount,0,0,-a.amount FROM posting_reversals r JOIN party_allocations a ON a.id=r.source_id AND r.source_type='party_allocation' JOIN party_funds f ON f.id=a.fund_id LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE f.side='{kind}'
 UNION ALL SELECT p.id,f.party_id,'refund',p.number,'party_refund',p.id,NULL,NULL,p.journal_id,p.created_at,0,-p.amount,(-({cash_sign}))*p.amount,0,0 FROM party_refunds p JOIN party_funds f ON f.id=p.fund_id WHERE f.side='{kind}'
 UNION ALL SELECT r.id,f.party_id,'refund_reversal',p.number,'reversal',r.id,NULL,NULL,j.id,r.created_at,0,p.amount,{cash_sign}*p.amount,0,0 FROM posting_reversals r JOIN party_refunds p ON p.id=r.source_id AND r.source_type='party_refund' JOIN party_funds f ON f.id=p.fund_id LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE f.side='{kind}')
 SELECT events.*,CASE event_type
  WHEN 'refund' THEN (SELECT p.fund_id FROM party_refunds p WHERE p.id=events.source_id)
  WHEN 'refund_reversal' THEN (SELECT p.fund_id FROM posting_reversals r JOIN party_refunds p ON p.id=r.source_id WHERE r.id=events.source_id AND r.source_type='party_refund')
  ELSE NULL::uuid END fund_id
 FROM events WHERE created_at<$2 AND ($1::uuid IS NULL OR party_id=$1) ORDER BY created_at,id,event_type"#,
        payment_fk = if kind == "customer" {
            "invoice_id"
        } else {
            "bill_id"
        },
        return_fk = if kind == "customer" {
            "invoice_id"
        } else {
            "bill_id"
        }
    )
}
fn documents_sql(kind: &str) -> String {
    if kind == "customer" {
        r#"SELECT 'invoice'::text target_type,i.id,i.number,i.customer_id party_id,c.name party_name,i.total,
 CASE WHEN t.id IS NULL THEN i.invoice_date ELSE t.invoice_date END document_date,CASE WHEN t.id IS NULL THEN i.due_date ELSE t.due_date END due_date,i.version
 FROM invoices i JOIN customers c ON c.id=i.customer_id LEFT JOIN LATERAL(SELECT * FROM invoice_terms_changes WHERE invoice_id=i.id AND created_at<$2 ORDER BY created_at DESC,id DESC LIMIT 1)t ON true
 WHERE i.created_at<$2 AND ($1::uuid IS NULL OR i.customer_id=$1)
 UNION ALL SELECT 'opening_receivable',o.id,o.reference,o.party_id,c.name,o.amount,o.document_date,o.due_date,COALESCE(v.version,1)
 FROM opening_party_items o JOIN customers c ON c.id=o.party_id LEFT JOIN party_target_versions v ON v.target_type='opening_receivable' AND v.target_id=o.id WHERE o.kind='receivable' AND o.created_at<$2 AND ($1::uuid IS NULL OR o.party_id=$1)"#.into()
    } else {
        r#"SELECT 'bill'::text target_type,b.id,b.number,b.supplier_id party_id,b.supplier_name party_name,b.total,b.invoice_date document_date,b.due_date,b.version::bigint version
 FROM purchase_bills b WHERE b.created_at<$2 AND ($1::uuid IS NULL OR b.supplier_id=$1)
 UNION ALL SELECT 'opening_payable',o.id,o.reference,o.party_id,s.name,o.amount,o.document_date,o.due_date,COALESCE(v.version,1)
 FROM opening_party_items o JOIN suppliers s ON s.id=o.party_id LEFT JOIN party_target_versions v ON v.target_type='opening_payable' AND v.target_id=o.id WHERE o.kind='payable' AND o.created_at<$2 AND ($1::uuid IS NULL OR o.party_id=$1)"#.into()
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StatementFilter {
    as_of: Option<String>,
    from: Option<String>,
}
async fn report_connection(pool: &PgPool) -> AppResult<Transaction<'_, Postgres>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}
async fn cutoff(
    tx: &mut Transaction<'_, Postgres>,
    as_of: Option<&str>,
) -> AppResult<(String, NaiveDate, chrono::DateTime<chrono::Utc>)> {
    let timezone: String = sqlx::query_scalar("SELECT timezone FROM company")
        .fetch_one(&mut **tx)
        .await?;
    let day = if let Some(day) = as_of {
        date(day)?
    } else {
        sqlx::query_scalar("SELECT (now() AT TIME ZONE $1)::date")
            .bind(&timezone)
            .fetch_one(&mut **tx)
            .await?
    };
    let end: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT ($1::date+1)::timestamp AT TIME ZONE $2")
            .bind(day)
            .bind(&timezone)
            .fetch_one(&mut **tx)
            .await?;
    Ok((timezone, day, end))
}
fn field(row: &sqlx::postgres::PgRow, key: &str) -> Decimal {
    row.get(key)
}
fn money_string(value: Decimal) -> String {
    format!("{value:.2}")
}
async fn target_rows(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    party: Option<Uuid>,
    day: NaiveDate,
    end: chrono::DateTime<chrono::Utc>,
) -> AppResult<Vec<Value>> {
    let events = sqlx::query(&events_sql(kind))
        .bind(party)
        .bind(end)
        .fetch_all(&mut **tx)
        .await?;
    let docs = sqlx::query(&documents_sql(kind))
        .bind(party)
        .bind(end)
        .fetch_all(&mut **tx)
        .await?;
    let mut output = Vec::new();
    for doc in docs {
        let id: Uuid = doc.get("id");
        let target_type: String = doc.get("target_type");
        let relevant: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get::<Option<Uuid>, _>("target_id") == Some(id)
                    && e.get::<Option<String>, _>("target_type").as_deref() == Some(&target_type)
            })
            .collect();
        let outstanding: Decimal = relevant.iter().map(|e| field(e, "debt_delta")).sum();
        if outstanding <= Decimal::ZERO {
            continue;
        }
        let credit: Decimal = relevant.iter().map(|e| field(e, "document_credit")).sum();
        let paid: Decimal = relevant.iter().map(|e| field(e, "settled_delta")).sum();
        let due: Option<NaiveDate> = doc.get("due_date");
        let overdue = due.map(|d| (day - d).num_days().max(0));
        let bucket = match overdue {
            None => "unknown",
            Some(0) => "current",
            Some(1..=30) => "days_1_30",
            Some(31..=60) => "days_31_60",
            Some(61..=90) => "days_61_90",
            _ => "over_90",
        };
        output.push(json!({"target_type":target_type,"id":id,"number":doc.get::<String,_>("number"),"party_id":doc.get::<Uuid,_>("party_id"),"party_name":doc.get::<String,_>("party_name"),"total":money_string(doc.get("total")),"credit_total":money_string(credit),"paid":money_string(paid),"outstanding":money_string(outstanding),"document_date":doc.get::<Option<NaiveDate>,_>("document_date"),"due_date":due,"version":doc.get::<i64,_>("version"),"bucket":bucket,"days_overdue":overdue}));
    }
    output.sort_by(|a, b| {
        a["due_date"]
            .as_str()
            .cmp(&b["due_date"].as_str())
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    Ok(output)
}
async fn targets(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PartyFilter>,
) -> AppResult<Json<Value>> {
    let kind = filter
        .side
        .ok_or_else(|| AppError::bad_request("Jenis pihak wajib dipilih"))?;
    side(&kind)?;
    let mut tx = report_connection(&state.db).await?;
    let (_, day, end) = cutoff(&mut tx, None).await?;
    let items = target_rows(&mut tx, &kind, filter.party_id, day, end).await?;
    tx.commit().await?;
    Ok(Json(json!({"items":items})))
}

async fn statement(
    State(state): State<AppState>,
    _user: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    Query(filter): Query<StatementFilter>,
) -> AppResult<Json<Value>> {
    side(&kind)?;
    let mut tx = report_connection(&state.db).await?;
    let name = party_name(&mut tx, &kind, id).await?;
    let (timezone, day, end) = cutoff(&mut tx, filter.as_of.as_deref()).await?;
    let from = filter.from.as_deref().map(date).transpose()?;
    if from.is_some_and(|v| v > day) {
        return Err(AppError::bad_request(
            "Tanggal awal aktivitas melewati tanggal posisi saldo",
        ));
    }
    let start: Option<chrono::DateTime<chrono::Utc>> = if let Some(d) = from {
        Some(
            sqlx::query_scalar("SELECT $1::date::timestamp AT TIME ZONE $2")
                .bind(d)
                .bind(&timezone)
                .fetch_one(&mut *tx)
                .await?,
        )
    } else {
        None
    };
    let rows = sqlx::query(&events_sql(&kind))
        .bind(Some(id))
        .bind(end)
        .fetch_all(&mut *tx)
        .await?;
    let mut debt = Decimal::ZERO;
    let mut credits = Decimal::ZERO;
    let mut opening_debt = Decimal::ZERO;
    let mut opening_credit = Decimal::ZERO;
    let mut events = Vec::new();
    for row in rows {
        let debt_delta = field(&row, "debt_delta");
        let credit_delta = field(&row, "credit_delta");
        debt += debt_delta;
        credits += credit_delta;
        let created: chrono::DateTime<chrono::Utc> = row.get("created_at");
        if start.is_some_and(|s| created < s) {
            opening_debt = debt;
            opening_credit = credits;
            continue;
        }
        let fund_id: Option<Uuid> = row.get("fund_id");
        events.push(json!({"id":row.get::<Uuid,_>("id"),"event_type":row.get::<String,_>("event_type"),"number":row.get::<String,_>("number"),"source_type":row.get::<String,_>("source_type"),"source_id":row.get::<Uuid,_>("source_id"),"fund_id":fund_id,"source_url":fund_id.map(|id|format!("?fund={id}#balances")),"target_type":row.get::<Option<String>,_>("target_type"),"target_id":row.get::<Option<Uuid>,_>("target_id"),"journal_id":row.get::<Option<Uuid>,_>("journal_id"),"created_at":created,"debt_delta":money_string(debt_delta),"credit_delta":money_string(credit_delta),"cash_delta":money_string(field(&row,"cash_delta")),"debt_balance":money_string(debt),"credit_balance":money_string(credits),"net_balance":money_string(debt-credits)}));
    }
    let documents = target_rows(&mut tx, &kind, Some(id), day, end).await?;
    let keys = [
        "current",
        "days_1_30",
        "days_31_60",
        "days_61_90",
        "over_90",
        "unknown",
    ];
    let aging: Vec<Value> = keys
        .iter()
        .map(|key| {
            let docs: Vec<_> = documents.iter().filter(|d| d["bucket"] == *key).collect();
            let total: Decimal = docs
                .iter()
                .map(|d| {
                    d["outstanding"]
                        .as_str()
                        .unwrap()
                        .parse::<Decimal>()
                        .unwrap()
                })
                .sum();
            json!({"key":key,"count":docs.len(),"total":money_string(total)})
        })
        .collect();
    let reserved:Decimal=sqlx::query_scalar(r#"SELECT COALESCE(sum(q.amount-COALESCE((SELECT sum(p.amount) FROM party_refunds p WHERE p.request_id=q.id AND p.created_at<$3 AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_refund' AND r.source_id=p.id AND r.created_at<$3)),0)),0)
 FROM party_refund_requests q JOIN party_funds f ON f.id=q.fund_id WHERE f.side=$1 AND f.party_id=$2 AND q.created_at<$3 AND NOT EXISTS(SELECT 1 FROM party_refund_cancellations c WHERE c.request_id=q.id AND c.created_at<$3)"#).bind(&kind).bind(id).bind(end).fetch_one(&mut *tx).await?;
    let all = sqlx::query(&events_sql(&kind))
        .bind(Option::<Uuid>::None)
        .bind(end)
        .fetch_all(&mut *tx)
        .await?;
    let all_debt: Decimal = all.iter().map(|r| field(r, "debt_delta")).sum();
    let all_credit: Decimal = all.iter().map(|r| field(r, "credit_delta")).sum();
    let obligation = obligation_account(&mut tx, &kind).await?;
    let control = party_control_account(&mut tx, &kind).await?;
    let ledger_debt:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(l.debit-l.credit),0)*$3 FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id WHERE l.account_code=$1 AND j.created_at<$2").bind(obligation).bind(end).bind(if kind=="customer"{1i32}else{-1i32}).fetch_one(&mut *tx).await?;
    let ledger_credit:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(l.debit-l.credit),0)*$3 FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id WHERE l.account_code=$1 AND j.created_at<$2").bind(control).bind(end).bind(if kind=="customer"{-1i32}else{1i32}).fetch_one(&mut *tx).await?;
    let result = json!({"side":kind,"party":{"id":id,"name":name},"timezone":timezone,"as_of":day,"from":from,"summary":{"outstanding":money_string(debt),"credit_balance":money_string(credits),"available_credit":money_string(credits-reserved),"reserved_refunds":money_string(reserved),"net_balance":money_string(debt-credits)},"opening":{"debt_balance":money_string(opening_debt),"credit_balance":money_string(opening_credit),"net_balance":money_string(opening_debt-opening_credit)},"events":events,"documents":documents,"aging":aging,"reconciliation":{"ar_or_ap_ledger":money_string(ledger_debt),"ar_or_ap_subledger":money_string(all_debt),"ar_or_ap_difference":money_string(ledger_debt-all_debt),"credits_ledger":money_string(ledger_credit),"credits_subledger":money_string(all_credit),"credits_difference":money_string(ledger_credit-all_credit),"balanced":ledger_debt==all_debt&&ledger_credit==all_credit}});
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TermsInput {
    version: i64,
    invoice_date: Option<String>,
    due_date: Option<String>,
    reason: String,
    idempotency_key: Uuid,
}
async fn update_terms(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<TermsInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let note = reason(&body.reason)?;
    let mut issue = body.invoice_date.as_deref().map(date).transpose()?;
    let due = body.due_date.as_deref().map(date).transpose()?;
    if issue.zip(due).is_some_and(|(a, b)| b < a) {
        return Err(AppError::bad_request(
            "Jatuh tempo tidak boleh mendahului tanggal invoice",
        ));
    }
    let request = payload("invoice.terms", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let target = lock_target(&mut tx, "invoice", id).await?;
    if target.version != body.version {
        return Err(AppError::conflict(
            "Invoice berubah; muat ulang sebelum mengubah tanggal dan jatuh tempo",
        ));
    }
    if issue.is_none() {
        issue=sqlx::query_scalar("SELECT CASE WHEN t.id IS NULL THEN i.invoice_date ELSE t.invoice_date END FROM invoices i LEFT JOIN LATERAL(SELECT * FROM invoice_terms_changes WHERE invoice_id=i.id ORDER BY created_at DESC,id DESC LIMIT 1)t ON true WHERE i.id=$1")
            .bind(id).fetch_one(&mut *tx).await?;
    }
    let today: NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date FROM company")
            .fetch_one(&mut *tx)
            .await?;
    if issue.is_some_and(|v| v > today) || issue.zip(due).is_some_and(|(a, b)| b < a) {
        return Err(AppError::bad_request(
            "Tanggal invoice tidak boleh di masa depan dan jatuh tempo tidak boleh mendahuluinya",
        ));
    }
    sqlx::query("INSERT INTO invoice_terms_changes(id,invoice_id,invoice_date,due_date,reason,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::new_v4()).bind(id).bind(issue).bind(due).bind(&note).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE invoices SET version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "invoice.terms_changed",
        "invoice",
        id,
        json!({"reason":note,"invoice_date":issue,"due_date":due}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(crate::business::INVOICE_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CustomerTermsInput {
    payment_terms_days: i32,
    previous_payment_terms_days: i32,
    reason: String,
    idempotency_key: Uuid,
}
async fn customer_terms(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CustomerTermsInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if !(0..=3650).contains(&body.payment_terms_days) {
        return Err(AppError::bad_request(
            "Jangka pembayaran harus antara 0 dan 3.650 hari",
        ));
    }
    let note = reason(&body.reason)?;
    let request = payload("customer.terms", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let previous: i32 =
        sqlx::query_scalar("SELECT payment_terms_days FROM customers WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    if previous != body.previous_payment_terms_days {
        return Err(AppError::conflict(
            "Jangka pembayaran pelanggan berubah; muat ulang sebelum melanjutkan",
        ));
    }
    let result: Value = sqlx::query_scalar(
        "UPDATE customers SET payment_terms_days=$2 WHERE id=$1 RETURNING to_jsonb(customers)",
    )
    .bind(id)
    .bind(body.payment_terms_days)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        &user,
        "customer.terms_changed",
        "customer",
        id,
        json!({"previous":previous,"payment_terms_days":body.payment_terms_days,"reason":note}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
