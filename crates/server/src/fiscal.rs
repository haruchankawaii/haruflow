use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember, text},
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
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::str::FromStr;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounting/fiscal", get(status))
        .route("/accounting/fiscal/preview", get(preview))
        .route("/accounting/fiscal/close", post(close_year))
        .route("/accounting/fiscal/{id}/reverse", post(reverse_year))
        .route("/accounting/period/reopen", post(reopen))
        .route("/accounting/dated-journals", post(dated_journal))
}

pub(crate) fn calendar(value: &str) -> AppResult<NaiveDate> {
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Tanggal harus YYYY-MM-DD."))?;
    if d.year() < 1 || d.year() > 9999 || d.format("%Y-%m-%d").to_string() != value {
        return Err(AppError::bad_request("Tanggal kalender tidak valid."));
    }
    Ok(d)
}
fn reason(value: &str) -> AppResult<String> {
    let v = text(value, 1000, "Alasan")?;
    if v.chars().count() < 3 {
        return Err(AppError::bad_request("Alasan minimal tiga karakter."));
    }
    Ok(v)
}
pub(crate) async fn check_date(
    tx: &mut Transaction<'_, Postgres>,
    date: NaiveDate,
) -> AppResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut **tx)
        .await?;
    let row=sqlx::query("SELECT (now() AT TIME ZONE c.timezone)::date today,p.closed_through FROM company c CROSS JOIN accounting_period p WHERE p.singleton").fetch_one(&mut **tx).await?;
    if date > row.get::<NaiveDate, _>("today") {
        return Err(AppError::bad_request(
            "Tanggal pembukuan tidak boleh di masa depan.",
        ));
    }
    if row
        .get::<Option<NaiveDate>, _>("closed_through")
        .is_some_and(|closed| date <= closed)
    {
        return Err(AppError::conflict(
            "Tanggal pembukuan berada dalam periode tertutup.",
        ));
    }
    Ok(())
}
pub(crate) async fn timestamp(
    tx: &mut Transaction<'_, Postgres>,
    date: NaiveDate,
) -> AppResult<chrono::DateTime<chrono::Utc>> {
    Ok(sqlx::query_scalar("SELECT CASE WHEN $1=(now() AT TIME ZONE timezone)::date THEN now() ELSE ($1::date+time '12:00') AT TIME ZONE timezone END FROM company").bind(date).fetch_one(&mut **tx).await?)
}
async fn read_status(db: &PgPool) -> AppResult<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('closed_through',p.closed_through,'version',p.version,'today',(now() AT TIME ZONE c.timezone)::date,'timezone',c.timezone,'events',COALESCE((SELECT jsonb_agg(to_jsonb(e)||jsonb_build_object('user_name',u.name) ORDER BY e.version DESC) FROM period_events e JOIN users u ON u.id=e.created_by),'[]'::jsonb),'closings',COALESCE((SELECT jsonb_agg(to_jsonb(f)||jsonb_build_object('net_profit',f.net_profit::text,'reversal',to_jsonb(r)) ORDER BY f.year DESC,f.created_at DESC) FROM fiscal_closings f LEFT JOIN fiscal_closing_reversals r ON r.closing_id=f.id),'[]'::jsonb)) FROM accounting_period p CROSS JOIN company c WHERE p.singleton").fetch_one(db).await?)
}
async fn status(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    Ok(Json(read_status(&state.db).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct YearFilter {
    year: i32,
}
async fn year_plan(tx: &mut Transaction<'_, Postgres>, year: i32) -> AppResult<Value> {
    let end = NaiveDate::from_ymd_opt(year, 12, 31)
        .filter(|d| d.year() > 0 && d.year() < 9999)
        .ok_or_else(|| AppError::bad_request("Tahun tidak valid."))?;
    let cfg=sqlx::query("SELECT p.version,p.closed_through,(now() AT TIME ZONE c.timezone)::date today,c.timezone FROM accounting_period p CROSS JOIN company c WHERE p.singleton").fetch_one(&mut **tx).await?;
    let today: NaiveDate = cfg.get("today");
    if end >= today {
        return Err(AppError::bad_request(
            "Hanya tahun yang sudah berakhir yang dapat ditutup.",
        ));
    }
    let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fiscal_closings f WHERE f.year>=$1 AND NOT EXISTS(SELECT 1 FROM fiscal_closing_reversals r WHERE r.closing_id=f.id))").bind(year).fetch_one(&mut **tx).await?;
    if blocked {
        return Err(AppError::conflict(
            "Tahun ini atau tahun berikutnya sudah ditutup. Balikkan penutupan terbaru terlebih dahulu.",
        ));
    }
    let timezone: String = cfg.get("timezone");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE (created_at AT TIME ZONE $1)::date<=$2",
    )
    .bind(&timezone)
    .bind(end)
    .fetch_one(&mut **tx)
    .await?;
    let rows=sqlx::query("SELECT a.code,a.name,a.kind,COALESCE(sum(l.debit-l.credit),0) balance FROM accounts a LEFT JOIN journal_lines l ON l.account_code=a.code LEFT JOIN journal_entries j ON j.id=l.journal_id WHERE a.kind IN ('revenue','expense') AND (j.id IS NULL OR (j.created_at AT TIME ZONE $1)::date<=$2) GROUP BY a.code,a.name,a.kind ORDER BY a.code").bind(&timezone).bind(end).fetch_all(&mut **tx).await?;
    let mut lines = Vec::new();
    let mut net = Decimal::ZERO;
    for row in rows {
        let balance: Decimal = row.get("balance");
        if balance == Decimal::ZERO {
            continue;
        }
        net -= balance;
        lines.push(json!({"account_code":row.get::<String,_>("code"),"account_name":row.get::<String,_>("name"),"debit":(-balance).max(Decimal::ZERO).to_string(),"credit":balance.max(Decimal::ZERO).to_string()}));
    }
    if lines.is_empty() {
        return Err(AppError::conflict(
            "Tidak ada saldo pendapatan/beban yang belum ditutup sampai tanggal tersebut.",
        ));
    }
    let retained: String =
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='retained_earnings'")
            .fetch_one(&mut **tx)
            .await?;
    if net != Decimal::ZERO {
        lines.push(json!({"account_code":retained,"account_name":"Saldo laba tahun sebelumnya","debit":(-net).max(Decimal::ZERO).to_string(),"credit":net.max(Decimal::ZERO).to_string()}));
    }
    Ok(
        json!({"year":year,"through":end,"period_version":cfg.get::<i64,_>("version"),"journal_count":count,"previous_closed_through":cfg.get::<Option<NaiveDate>,_>("closed_through"),"net_profit":net.to_string(),"lines":lines}),
    )
}
async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Query(filter): Query<YearFilter>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ,READ ONLY")
        .execute(&mut *tx)
        .await?;
    let result = year_plan(&mut tx, filter.year).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CloseYear {
    year: i32,
    period_version: i64,
    journal_count: i64,
    reason: String,
    confirmed: bool,
    idempotency_key: Uuid,
}
async fn close_year(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CloseYear>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi penutupan tahun diperlukan.",
        ));
    }
    let reason = reason(&body.reason)?;
    let request = payload("fiscal.close", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    let plan = year_plan(&mut tx, body.year).await?;
    if plan["period_version"].as_i64() != Some(body.period_version)
        || plan["journal_count"].as_i64() != Some(body.journal_count)
    {
        return Err(AppError::conflict(
            "Pembukuan berubah; muat ulang preview penutupan.",
        ));
    }
    let end = calendar(plan["through"].as_str().unwrap())?;
    let id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at,reporting_kind) SELECT $1,$2,'year_close',$3,$4,($5::date+time '23:59:59.999999') AT TIME ZONE timezone,'year_close' FROM company").bind(journal_id).bind(format!("Tutup buku {}: {}",body.year,reason)).bind(id).bind(user.id).bind(end).execute(&mut *tx).await?;
    for line in plan["lines"].as_array().unwrap() {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)").bind(Uuid::new_v4()).bind(journal_id).bind(line["account_code"].as_str().unwrap()).bind(Decimal::from_str(line["debit"].as_str().unwrap()).unwrap()).bind(Decimal::from_str(line["credit"].as_str().unwrap()).unwrap()).execute(&mut *tx).await?;
    }
    let previous: Option<NaiveDate> = plan["previous_closed_through"]
        .as_str()
        .map(calendar)
        .transpose()?;
    sqlx::query("INSERT INTO fiscal_closings(id,year,journal_id,previous_closed_through,net_profit,reason,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(body.year).bind(journal_id).bind(previous).bind(Decimal::from_str(plan["net_profit"].as_str().unwrap()).unwrap()).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    let through = previous.map_or(end, |date| date.max(end));
    record_period(
        &mut tx,
        &user,
        "year_close",
        previous,
        Some(through),
        &reason,
    )
    .await?;
    audit(
        &mut tx,
        &user,
        "fiscal.closed",
        "fiscal_closing",
        id,
        json!({"plan":plan,"reason":reason,"journal_id":journal_id}),
    )
    .await?;
    let result = json!({"id":id,"journal_id":journal_id,"year":body.year,"closed_through":through,"net_profit":plan["net_profit"]});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn record_period(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    action: &str,
    previous: Option<NaiveDate>,
    through: Option<NaiveDate>,
    reason: &str,
) -> AppResult<i64> {
    let version:i64=sqlx::query_scalar("UPDATE accounting_period SET closed_through=$1,closed_by=$2,reason=$3,closed_at=now(),version=version+1 WHERE singleton RETURNING version").bind(through).bind(user.id).bind(reason).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO period_events(id,action,previous_closed_through,closed_through,reason,version,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(action).bind(previous).bind(through).bind(reason).bind(version).bind(user.id).execute(&mut **tx).await?;
    Ok(version)
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reopen {
    closed_through: Option<String>,
    version: i64,
    reason: String,
    confirmed: bool,
    idempotency_key: Uuid,
}
async fn reopen(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Reopen>,
) -> AppResult<Json<Value>> {
    user.require(&["admin"])?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi pembukaan kembali periode diperlukan.",
        ));
    }
    let reason = reason(&body.reason)?;
    let through = body.closed_through.as_deref().map(calendar).transpose()?;
    let request = payload("period.reopen", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query(
        "SELECT closed_through,version FROM accounting_period WHERE singleton FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    let previous: Option<NaiveDate> = row.get("closed_through");
    if row.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict("Versi periode berubah; muat ulang."));
    }
    if previous.is_none() || through.is_some_and(|d| Some(d) >= previous) {
        return Err(AppError::bad_request(
            "Batas baru harus lebih awal dari batas tertutup sebelumnya.",
        ));
    }
    let last:Option<i32>=sqlx::query_scalar("SELECT max(year) FROM fiscal_closings f WHERE NOT EXISTS(SELECT 1 FROM fiscal_closing_reversals r WHERE r.closing_id=f.id)").fetch_one(&mut *tx).await?;
    if last.is_some_and(|year| {
        through.is_none_or(|date| date < NaiveDate::from_ymd_opt(year, 12, 31).unwrap())
    }) {
        return Err(AppError::conflict(
            "Balikkan penutupan tahun terkait sebelum membuka periodenya.",
        ));
    }
    record_period(&mut tx, &user, "reopen", previous, through, &reason).await?;
    let id = Uuid::new_v4();
    audit(
        &mut tx,
        &user,
        "period.reopened",
        "accounting_period",
        id,
        json!({"previous":previous,"through":through,"reason":reason}),
    )
    .await?;
    let result = json!({"closed_through":through,"version":body.version+1});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseYear {
    period_version: i64,
    reason: String,
    confirmed: bool,
    idempotency_key: Uuid,
}
async fn reverse_year(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseYear>,
) -> AppResult<Json<Value>> {
    user.require(&["admin"])?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi pembalikan tutup buku diperlukan.",
        ));
    }
    let reason = reason(&body.reason)?;
    let request = payload("fiscal.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    let row=sqlx::query("SELECT f.*,j.created_at posting_date FROM fiscal_closings f JOIN journal_entries j ON j.id=f.journal_id WHERE f.id=$1").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    let year: i32 = row.get("year");
    let period = sqlx::query(
        "SELECT closed_through,version FROM accounting_period WHERE singleton FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    if period.get::<i64, _>("version") != body.period_version {
        return Err(AppError::conflict("Versi periode berubah."));
    }
    let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fiscal_closing_reversals WHERE closing_id=$1) OR EXISTS(SELECT 1 FROM fiscal_closings f WHERE f.year>$2 AND NOT EXISTS(SELECT 1 FROM fiscal_closing_reversals r WHERE r.closing_id=f.id))").bind(id).bind(year).fetch_one(&mut *tx).await?;
    if blocked {
        return Err(AppError::conflict(
            "Penutupan sudah dibalik atau ada penutupan tahun berikutnya yang harus dibalik terlebih dahulu.",
        ));
    }
    let previous: Option<NaiveDate> = period.get("closed_through");
    let restored: Option<NaiveDate> = row.get("previous_closed_through");
    if previous != Some(restored.map_or(end, |date| date.max(end))) {
        return Err(AppError::conflict(
            "Batas periode berubah sejak tutup buku; sesuaikan batas periode terlebih dahulu.",
        ));
    }
    let reversal = Uuid::new_v4();
    let journal = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at,reporting_kind) VALUES($1,$2,'year_close',$3,$4,$5,'year_close')").bind(journal).bind(format!("Pembalikan tutup buku {year}: {reason}")).bind(reversal).bind(user.id).bind(row.get::<chrono::DateTime<chrono::Utc>,_>("posting_date")).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$1,account_code,credit,debit FROM journal_lines WHERE journal_id=$2").bind(journal).bind(row.get::<Uuid,_>("journal_id")).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO fiscal_closing_reversals(id,closing_id,journal_id,reason,created_by) VALUES($1,$2,$3,$4,$5)").bind(reversal).bind(id).bind(journal).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    record_period(
        &mut tx,
        &user,
        "year_close_reversal",
        previous,
        restored,
        &reason,
    )
    .await?;
    audit(
        &mut tx,
        &user,
        "fiscal.reversed",
        "fiscal_closing",
        id,
        json!({"journal_id":journal,"reason":reason}),
    )
    .await?;
    let result = json!({"id":reversal,"journal_id":journal,"closed_through":restored});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatedLine {
    pub account_code: String,
    pub debit: String,
    pub credit: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DatedJournal {
    posting_date: String,
    description: String,
    reason: String,
    lines: Vec<DatedLine>,
    confirmed: bool,
    idempotency_key: Uuid,
}
pub(crate) fn line_value(value: &str) -> AppResult<Decimal> {
    let d = Decimal::from_str(value)
        .map_err(|_| AppError::bad_request("Nominal jurnal tidak valid."))?;
    if d < Decimal::ZERO || d > Decimal::from(1_000_000_000_000u64) || d.normalize().scale() > 2 {
        return Err(AppError::bad_request(
            "Nominal harus nonnegatif, maksimal satu triliun, dua desimal.",
        ));
    }
    Ok(d)
}
pub(crate) async fn insert_dated_journal(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    date: NaiveDate,
    description: &str,
    lines: &[DatedLine],
) -> AppResult<Uuid> {
    check_date(tx, date).await?;
    if !(2..=100).contains(&lines.len()) {
        return Err(AppError::bad_request("Jurnal memerlukan 2–100 baris."));
    }
    let mut total = Decimal::ZERO;
    let mut debit = Decimal::ZERO;
    let mut parsed = Vec::new();
    for line in lines {
        let d = line_value(&line.debit)?;
        let c = line_value(&line.credit)?;
        if (d > Decimal::ZERO) == (c > Decimal::ZERO) {
            return Err(AppError::bad_request(
                "Setiap baris memerlukan satu sisi debit atau kredit positif.",
            ));
        }
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE code=$1 AND code NOT IN ('1200','1300') AND system_role IS NULL)").bind(&line.account_code).fetch_one(&mut **tx).await?;
        if !allowed {
            return Err(AppError::bad_request(
                "Akun tidak tersedia atau merupakan akun kontrol; gunakan dokumen sumber.",
            ));
        }
        total += d - c;
        debit += d;
        parsed.push((line, d, c));
    }
    if total != Decimal::ZERO || debit > Decimal::from(1_000_000_000_000u64) {
        return Err(AppError::bad_request(
            "Jurnal harus seimbang dan maksimal satu triliun.",
        ));
    }
    let id = Uuid::new_v4();
    let at = timestamp(tx, date).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) VALUES($1,$2,'manual',$1,$3,$4)").bind(id).bind(description).bind(user.id).bind(at).execute(&mut **tx).await?;
    for (line, d, c) in parsed {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)").bind(Uuid::new_v4()).bind(id).bind(&line.account_code).bind(d).bind(c).execute(&mut **tx).await?;
    }
    Ok(id)
}
async fn dated_journal(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<DatedJournal>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi tanggal pembukuan diperlukan.",
        ));
    }
    let date = calendar(&body.posting_date)?;
    let reason = reason(&body.reason)?;
    let description = text(&body.description, 500, "Keterangan")?;
    let request = payload("journal.dated", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let id = insert_dated_journal(&mut tx, &user, date, &description, &body.lines).await?;
    audit(
        &mut tx,
        &user,
        "journal.dated",
        "journal",
        id,
        json!({"posting_date":date,"reason":reason,"description":description}),
    )
    .await?;
    let result = json!({"id":id,"posting_date":date});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
