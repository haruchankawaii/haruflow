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
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post},
};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/treasury/accounts", get(accounts).post(create_account))
        .route("/treasury/transfers", get(transfers).post(transfer))
        .route("/treasury/transfers/{id}/reverse", post(reverse_transfer))
        .route(
            "/treasury/statements/preview",
            post(preview).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/treasury/statements/import",
            post(import).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/treasury/reconciliation", get(reconciliation))
        .route("/treasury/statement-lines/{id}/match", post(match_line))
        .route("/treasury/statement-lines/{id}/unmatch", post(unmatch_line))
}

pub(crate) async fn cash_account(
    tx: &mut Transaction<'_, Postgres>,
    requested: Option<&str>,
) -> AppResult<String> {
    let code = requested.unwrap_or("1100");
    sqlx::query_scalar("SELECT account_code FROM treasury_accounts WHERE account_code=$1")
        .bind(code)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::bad_request("Pilih akun kas atau bank yang terdaftar"))
}
pub(crate) async fn require_balance(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
    value: Decimal,
) -> AppResult<()> {
    let balance: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code=$1",
    )
    .bind(code)
    .fetch_one(&mut **tx)
    .await?;
    if balance < value {
        return Err(AppError::conflict(format!(
            "Saldo akun {code} tidak mencukupi"
        )));
    }
    Ok(())
}
pub(crate) async fn journal_cash_account(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<String> {
    let rows:Vec<String>=sqlx::query_scalar("SELECT DISTINCT l.account_code FROM journal_lines l JOIN treasury_accounts t ON t.account_code=l.account_code WHERE l.journal_id=$1").bind(id).fetch_all(&mut **tx).await?;
    if rows.len() != 1 {
        return Err(AppError::conflict(
            "Akun kas sumber tidak tunggal; periksa jurnal asal",
        ));
    }
    Ok(rows[0].clone())
}
fn clean(value: &str, max: usize, label: &str) -> AppResult<String> {
    let value = text(value, max, label)?;
    if value.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "{label} tidak boleh berisi karakter kontrol"
        )));
    }
    Ok(value)
}
fn note(value: &str) -> AppResult<String> {
    let s = clean(value, 1000, "Alasan")?;
    if s.chars().count() < 3 {
        return Err(AppError::bad_request("Alasan minimal 3 karakter"));
    }
    Ok(s)
}
fn optional(value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| clean(v, max, "Referensi"))
        .transpose()
}
async fn accounts(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(t)||jsonb_build_object('name',a.name,'balance',COALESCE((SELECT sum(debit-credit) FROM journal_lines WHERE account_code=t.account_code),0)::text) ORDER BY t.account_code),'[]'::jsonb) FROM treasury_accounts t JOIN accounts a ON a.code=t.account_code").fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AccountInput {
    account_code: String,
    name: String,
    kind: String,
    bank_name: Option<String>,
    account_number: Option<String>,
    idempotency_key: Uuid,
}
async fn create_account(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AccountInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let code = clean(&body.account_code, 10, "Kode akun")?;
    if !(4..=10).contains(&code.len())
        || !code.bytes().all(|b| b.is_ascii_digit())
        || !matches!(body.kind.as_str(), "cash" | "bank")
    {
        return Err(AppError::bad_request(
            "Kode akun harus 4–10 digit; jenis cash atau bank",
        ));
    }
    let name = clean(&body.name, 200, "Nama akun")?;
    let bank = optional(body.bank_name.as_deref(), 200)?;
    let number = optional(body.account_number.as_deref(), 100)?;
    let request = payload("treasury.account.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM accounts WHERE code=$1)")
        .bind(&code)
        .fetch_one(&mut *tx)
        .await?
    {
        return Err(AppError::conflict(
            "Kode akun sudah digunakan. Gunakan kode baru agar makna riwayat lama tetap utuh",
        ));
    }
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES($1,$2,'asset')")
        .bind(&code)
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO treasury_accounts(account_code,kind,bank_name,account_number,created_by) VALUES($1,$2,$3,$4,$5)").bind(&code).bind(&body.kind).bind(bank).bind(number).bind(user.id).execute(&mut *tx).await?;
    let result =
        json!({"account_code":code,"name":name,"kind":body.kind,"currency":"IDR","balance":"0"});
    audit(
        &mut tx,
        &user,
        "treasury.account.created",
        "account",
        Uuid::nil(),
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn transfer_document(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Value> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('amount',amount::text,'version',CASE WHEN r.id IS NULL THEN 1 ELSE 2 END,'reversal',CASE WHEN r.id IS NULL THEN NULL ELSE to_jsonb(r) END) FROM treasury_transfers t LEFT JOIN posting_reversals r ON r.source_type='treasury_transfer' AND r.source_id=t.id WHERE t.id=$1").bind(id).fetch_one(&mut **tx).await?)
}
async fn transfers(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(doc ORDER BY created_at DESC,id DESC),'[]'::jsonb) FROM(SELECT t.id,t.created_at,to_jsonb(t)||jsonb_build_object('amount',amount::text,'version',CASE WHEN r.id IS NULL THEN 1 ELSE 2 END,'reversal',CASE WHEN r.id IS NULL THEN NULL ELSE to_jsonb(r) END) doc FROM treasury_transfers t LEFT JOIN posting_reversals r ON r.source_type='treasury_transfer' AND r.source_id=t.id ORDER BY t.created_at DESC,t.id DESC LIMIT 500) x").fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TransferInput {
    from_account: String,
    to_account: String,
    amount: String,
    reference: Option<String>,
    reason: String,
    idempotency_key: Uuid,
}
async fn transfer(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<TransferInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let amount = domain::payment_amount(&body.amount).map_err(AppError::bad_request)?;
    let reason = note(&body.reason)?;
    let reference = optional(body.reference.as_deref(), 200)?;
    if body.from_account == body.to_account {
        return Err(AppError::bad_request(
            "Rekening sumber dan tujuan harus berbeda",
        ));
    }
    let request = payload("treasury.transfer", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let from = cash_account(&mut tx, Some(&body.from_account)).await?;
    let to = cash_account(&mut tx, Some(&body.to_account)).await?;
    require_balance(&mut tx, &from, amount).await?;
    let id = Uuid::new_v4();
    journal(
        &mut tx,
        &user,
        "treasury_transfer",
        id,
        &format!("Transfer {from} → {to}: {reason}"),
        (&to, &from),
        amount,
    )
    .await?;
    let journal_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM journal_entries WHERE source_type='treasury_transfer' AND source_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let number: String =
        sqlx::query_scalar("SELECT 'TR-'||nextval('treasury_transfer_number')::text")
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO treasury_transfers(id,number,from_account,to_account,amount,reference,reason,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(number).bind(from).bind(to).bind(amount).bind(reference).bind(reason).bind(journal_id).bind(user.id).execute(&mut *tx).await?;
    let result = transfer_document(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "treasury.transfer.created",
        "treasury_transfer",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}
async fn reverse_transfer(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = note(&body.reason)?;
    let request = payload(
        "treasury.transfer.reverse",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row = sqlx::query("SELECT * FROM treasury_transfers WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let reversed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='treasury_transfer' AND source_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    if reversed || body.version != 1 {
        return Err(AppError::conflict("Transfer sudah berubah atau dibalik"));
    }
    let amount: Decimal = row.get("amount");
    let from: String = row.get("from_account");
    let to: String = row.get("to_account");
    require_balance(&mut tx, &to, amount).await?;
    let reversal = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'treasury_transfer',$2,$3,$4)").bind(reversal).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    journal(
        &mut tx,
        &user,
        "reversal",
        reversal,
        &format!("Pembalikan transfer: {reason}"),
        (&from, &to),
        amount,
    )
    .await?;
    let result = transfer_document(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "treasury.transfer.reversed",
        "treasury_transfer",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StatementInput {
    account_code: String,
    reference: String,
    from: String,
    to: String,
    opening_balance: String,
    closing_balance: String,
    csv: String,
    delimiter: Option<String>,
    sha256: Option<String>,
    idempotency_key: Option<Uuid>,
}
#[derive(Serialize)]
struct StatementRow {
    row_number: i32,
    transaction_date: NaiveDate,
    reference: String,
    description: String,
    amount: String,
}
fn date(s: &str) -> AppResult<NaiveDate> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Tanggal harus YYYY-MM-DD"))?;
    if !(1..=9999).contains(&d.year()) || d.to_string() != s {
        return Err(AppError::bad_request("Tanggal harus YYYY-MM-DD"));
    }
    Ok(d)
}
fn signed_amount(s: &str) -> AppResult<Decimal> {
    let sign = if s.starts_with('-') {
        -Decimal::ONE
    } else {
        Decimal::ONE
    };
    let a =
        domain::payment_amount(s.strip_prefix('-').unwrap_or(s)).map_err(AppError::bad_request)?;
    Ok(a * sign)
}
fn balance(s: &str) -> AppResult<Decimal> {
    if s.parse::<Decimal>()
        .is_ok_and(|v| v == Decimal::ZERO && v.scale() <= 2)
    {
        Ok(Decimal::ZERO)
    } else {
        signed_amount(s)
    }
}
fn statement_plan(
    body: &StatementInput,
) -> AppResult<(Vec<StatementRow>, Decimal, Decimal, String)> {
    if body.csv.len() > 256 * 1024 || body.csv.contains('\0') {
        return Err(AppError::bad_request("CSV maksimal 256 KiB dan tanpa NUL"));
    }
    let delimiter = match body.delimiter.as_deref().unwrap_or(",") {
        "," => ',',
        ";" => ';',
        _ => {
            return Err(AppError::bad_request(
                "Pemisah CSV harus koma atau titik koma",
            ));
        }
    };
    let from = date(&body.from)?;
    let to = date(&body.to)?;
    if from > to {
        return Err(AppError::bad_request("Rentang tanggal terbalik"));
    }
    clean(&body.reference, 200, "Referensi mutasi")?;
    let records = parse_csv(&body.csv, delimiter)?;
    if records.first().is_none_or(|r| {
        r != &vec![
            "date".to_owned(),
            "reference".to_owned(),
            "description".to_owned(),
            "amount".to_owned(),
        ]
    }) {
        return Err(AppError::bad_request(
            "Header CSV harus date,reference,description,amount",
        ));
    }
    if records.len() < 2 {
        return Err(AppError::bad_request(
            "CSV harus berisi minimal satu transaksi",
        ));
    }
    let mut rows = Vec::new();
    let mut refs = std::collections::BTreeSet::new();
    let mut total = Decimal::ZERO;
    for (index, r) in records.into_iter().skip(1).enumerate() {
        if r.len() != 4 {
            return Err(AppError::bad_request(format!(
                "Baris {} harus memiliki empat kolom",
                index + 2
            )));
        }
        let transaction_date = date(&r[0])?;
        if transaction_date < from || transaction_date > to {
            return Err(AppError::bad_request(format!(
                "Tanggal baris {} di luar periode mutasi",
                index + 2
            )));
        }
        let reference = clean(&r[1], 200, "Referensi transaksi")?;
        if !refs.insert(reference.clone()) {
            return Err(AppError::bad_request("Referensi transaksi ganda dalam CSV"));
        }
        let description = clean(&r[2], 1000, "Deskripsi")?;
        let amount = signed_amount(&r[3])?;
        total += amount;
        rows.push(StatementRow {
            row_number: index as i32 + 1,
            transaction_date,
            reference,
            description,
            amount: amount.to_string(),
        });
    }
    let opening = balance(&body.opening_balance)?;
    let closing = balance(&body.closing_balance)?;
    if opening + total != closing {
        return Err(AppError::bad_request(
            "Saldo awal + mutasi harus sama dengan saldo akhir bank",
        ));
    }
    Ok((
        rows,
        opening,
        closing,
        hex::encode(Sha256::digest(body.csv.as_bytes())),
    ))
}
// Bounded quoted CSV parser; no locale-dependent number guessing.
fn parse_csv(input: &str, delimiter: char) -> AppResult<Vec<Vec<String>>> {
    let mut chars = input
        .strip_prefix('\u{feff}')
        .unwrap_or(input)
        .chars()
        .peekable();
    let mut rows = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut after = false;
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => {
                    quoted = false;
                    after = true;
                }
                _ => field.push(ch),
            }
            continue;
        }
        if ch == delimiter {
            fields.push(std::mem::take(&mut field));
            after = false;
        } else if ch == '\n' || ch == '\r' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            fields.push(std::mem::take(&mut field));
            if fields.len() > 1 || !fields[0].is_empty() {
                rows.push(std::mem::take(&mut fields));
            } else {
                fields.clear();
            }
            after = false;
        } else if ch == '"' && field.is_empty() && !after {
            quoted = true;
        } else {
            if ch == '"' || after {
                return Err(AppError::bad_request("Kutip CSV tidak valid"));
            }
            field.push(ch);
        }
        if rows.len() > 1001 {
            return Err(AppError::bad_request("Maksimal 1.000 transaksi per CSV"));
        }
    }
    if quoted {
        return Err(AppError::bad_request("Kutip CSV belum ditutup"));
    }
    if !field.is_empty() || !fields.is_empty() || after {
        fields.push(field);
        rows.push(fields);
    }
    if rows.len() > 1001 {
        return Err(AppError::bad_request("Maksimal 1.000 transaksi per CSV"));
    }
    Ok(rows)
}
async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<StatementInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let (rows, opening, closing, sha) = statement_plan(&body)?;
    let mut tx = state.db.begin().await?;
    cash_account(&mut tx, Some(&body.account_code)).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"rows":rows,"opening_balance":opening.to_string(),"closing_balance":closing.to_string(),"sha256":sha,"count":rows.len(),"cash_effect":"0","valid":true}),
    ))
}
async fn import(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<StatementInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let (rows, opening, closing, sha) = statement_plan(&body)?;
    if body.sha256.as_deref() != Some(&sha) {
        return Err(AppError::bad_request(
            "Pratinjau CSV dahulu dan kirim SHA-256 yang sama",
        ));
    }
    let key = body
        .idempotency_key
        .ok_or_else(|| AppError::bad_request("Kunci idempotensi wajib"))?;
    let request = payload("treasury.statement.import", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    cash_account(&mut tx, Some(&body.account_code)).await?;
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM bank_statements WHERE account_code=$1 AND(reference=$2 OR file_sha256=$3))").bind(&body.account_code).bind(body.reference.trim()).bind(&sha).fetch_one(&mut *tx).await?;
    if duplicate {
        return Err(AppError::conflict("Mutasi bank ini sudah diimpor"));
    }
    let overlap:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM bank_statements WHERE account_code=$1 AND from_date<=$3 AND to_date>=$2)").bind(&body.account_code).bind(date(&body.from)?).bind(date(&body.to)?).fetch_one(&mut *tx).await?;
    if overlap {
        return Err(AppError::conflict(
            "Periode mutasi tumpang tindih dengan impor terdahulu; gunakan periode berikutnya",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO bank_statements(id,account_code,reference,from_date,to_date,opening_balance,closing_balance,file_sha256,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(&body.account_code).bind(body.reference.trim()).bind(date(&body.from)?).bind(date(&body.to)?).bind(opening).bind(closing).bind(&sha).bind(user.id).execute(&mut *tx).await?;
    for row in &rows {
        let line = Uuid::new_v4();
        sqlx::query("INSERT INTO bank_statement_lines(id,statement_id,row_number,transaction_date,reference,description,amount) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(line).bind(id).bind(row.row_number).bind(row.transaction_date).bind(&row.reference).bind(&row.description).bind(signed_amount(&row.amount)?).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO bank_line_versions(line_id) VALUES($1)")
            .bind(line)
            .execute(&mut *tx)
            .await?;
    }
    let result = json!({"id":id,"account_code":body.account_code,"count":rows.len(),"sha256":sha,"cash_effect":"0"});
    audit(
        &mut tx,
        &user,
        "treasury.statement.imported",
        "bank_statement",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconcileFilter {
    account_code: String,
}
async fn reconciliation(
    State(state): State<AppState>,
    user: AuthUser,
    Query(filter): Query<ReconcileFilter>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ,READ ONLY")
        .execute(&mut *tx)
        .await?;
    cash_account(&mut tx, Some(&filter.account_code)).await?;
    let statements:Value=sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(doc ORDER BY from_date DESC,id DESC),'[]'::jsonb) FROM(
 SELECT s.id,s.from_date,to_jsonb(s)||jsonb_build_object('opening_balance',s.opening_balance::text,'closing_balance',s.closing_balance::text,
 'ledger_opening',COALESCE((SELECT sum(l.debit-l.credit) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c WHERE l.account_code=s.account_code AND (j.created_at AT TIME ZONE c.timezone)::date<s.from_date),0)::text,
 'ledger_closing',COALESCE((SELECT sum(l.debit-l.credit) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c WHERE l.account_code=s.account_code AND (j.created_at AT TIME ZONE c.timezone)::date<=s.to_date),0)::text,
 'unmatched_bank',(SELECT count(*) FROM bank_statement_lines b LEFT JOIN bank_reconciliation_matches m ON m.line_id=b.id WHERE b.statement_id=s.id AND m.line_id IS NULL),
 'unmatched_ledger',(SELECT count(*) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c LEFT JOIN bank_reconciliation_matches m ON m.journal_line_id=l.id WHERE l.account_code=s.account_code AND (j.created_at AT TIME ZONE c.timezone)::date BETWEEN s.from_date AND s.to_date AND m.line_id IS NULL)) doc FROM bank_statements s WHERE s.account_code=$1) x"#).bind(&filter.account_code).fetch_one(&mut *tx).await?;
    let lines:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(b)||jsonb_build_object('amount',b.amount::text,'version',v.version,'statement_reference',s.reference,'journal_line_id',m.journal_line_id,'journal_id',l.journal_id,'history',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.version DESC) FROM bank_reconciliation_events e WHERE e.line_id=b.id),'[]'::jsonb)) ORDER BY b.transaction_date DESC,b.id DESC),'[]'::jsonb) FROM bank_statement_lines b JOIN bank_statements s ON s.id=b.statement_id JOIN bank_line_versions v ON v.line_id=b.id LEFT JOIN bank_reconciliation_matches m ON m.line_id=b.id LEFT JOIN journal_lines l ON l.id=m.journal_line_id WHERE s.account_code=$1").bind(&filter.account_code).fetch_one(&mut *tx).await?;
    let ledger:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('id',l.id,'journal_id',j.id,'date',(j.created_at AT TIME ZONE c.timezone)::date::text,'description',j.description,'amount',(l.debit-l.credit)::text,'matched',m.line_id IS NOT NULL) ORDER BY j.created_at DESC,l.id DESC),'[]'::jsonb) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c LEFT JOIN bank_reconciliation_matches m ON m.journal_line_id=l.id WHERE l.account_code=$1").bind(&filter.account_code).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"account_code":filter.account_code,"statements":statements,"lines":lines,"ledger":ledger}),
    ))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MatchInput {
    version: i64,
    journal_line_id: Option<Uuid>,
    reason: String,
    idempotency_key: Uuid,
}
async fn match_line(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MatchInput>,
) -> AppResult<Json<Value>> {
    change_match(state, user, id, body, true).await
}
async fn unmatch_line(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MatchInput>,
) -> AppResult<Json<Value>> {
    change_match(state, user, id, body, false).await
}
async fn change_match(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    body: MatchInput,
    matching: bool,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = note(&body.reason)?;
    let action = if matching { "match" } else { "unmatch" };
    let request = payload(
        "treasury.reconciliation.change",
        &user,
        &json!({"id":id,"action":action,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row=sqlx::query("SELECT b.amount,s.account_code,v.version FROM bank_statement_lines b JOIN bank_statements s ON s.id=b.statement_id JOIN bank_line_versions v ON v.line_id=b.id WHERE b.id=$1 FOR UPDATE OF v").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    let version: i64 = row.get("version");
    if version != body.version {
        return Err(AppError::conflict("Pencocokan sudah berubah; muat ulang"));
    }
    let old: Option<Uuid> = sqlx::query_scalar(
        "SELECT journal_line_id FROM bank_reconciliation_matches WHERE line_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let journal_line = if matching {
        if old.is_some() {
            return Err(AppError::conflict("Baris mutasi sudah dicocokkan"));
        }
        let target = body
            .journal_line_id
            .ok_or_else(|| AppError::bad_request("Pilih baris jurnal"))?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM journal_lines WHERE id=$1 AND account_code=$2 AND debit-credit=$3)").bind(target).bind(row.get::<String,_>("account_code")).bind(row.get::<Decimal,_>("amount")).fetch_one(&mut *tx).await?;
        if !valid {
            return Err(AppError::conflict(
                "Akun atau nominal jurnal tidak sama dengan mutasi bank",
            ));
        }
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM bank_reconciliation_matches WHERE journal_line_id=$1)",
        )
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        if taken {
            return Err(AppError::conflict(
                "Baris jurnal sudah dipakai oleh mutasi lain",
            ));
        }
        sqlx::query("INSERT INTO bank_reconciliation_matches(line_id,journal_line_id,created_by) VALUES($1,$2,$3)").bind(id).bind(target).bind(user.id).execute(&mut *tx).await?;
        target
    } else {
        let target = old.ok_or_else(|| AppError::conflict("Baris mutasi belum dicocokkan"))?;
        sqlx::query("DELETE FROM bank_reconciliation_matches WHERE line_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        target
    };
    sqlx::query("UPDATE bank_line_versions SET version=version+1 WHERE line_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO bank_reconciliation_events(id,line_id,journal_line_id,action,reason,version,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(id).bind(journal_line).bind(action).bind(reason).bind(version+1).bind(user.id).execute(&mut *tx).await?;
    let result = json!({"id":id,"version":version+1,"matched":matching,"journal_line_id":if matching{Some(journal_line)}else{None},"cash_effect":"0"});
    audit(
        &mut tx,
        &user,
        "treasury.reconciliation.changed",
        "bank_statement_line",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
