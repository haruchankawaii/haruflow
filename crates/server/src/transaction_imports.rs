use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    error::{AppError, AppResult},
    fiscal,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    routing::{get, post},
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transaction-imports", get(history))
        .route("/transaction-imports/{kind}/template", get(template))
        .route(
            "/transaction-imports/{kind}/preview",
            post(preview).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/transaction-imports/{id}", get(detail))
        .route("/transaction-imports/{id}/commit", post(commit))
        .route("/historical-documents", get(documents))
        .route("/historical-documents/{id}", get(document))
        .route("/historical-documents/{id}/reverse", post(reverse_document))
        .route("/historical-payments/{id}/reverse", post(reverse_payment))
}

#[derive(Clone, Copy)]
enum Kind {
    CustomerInvoices,
    SupplierBills,
    CustomerPayments,
    SupplierPayments,
    Journals,
}
impl Kind {
    fn parse(s: &str) -> AppResult<Self> {
        match s {
            "customer_invoices" => Ok(Self::CustomerInvoices),
            "supplier_bills" => Ok(Self::SupplierBills),
            "customer_payments" => Ok(Self::CustomerPayments),
            "supplier_payments" => Ok(Self::SupplierPayments),
            "journals" => Ok(Self::Journals),
            _ => Err(AppError::bad_request(
                "Jenis impor transaksi tidak tersedia.",
            )),
        }
    }
    fn side(self) -> &'static str {
        match self {
            Self::CustomerInvoices | Self::CustomerPayments => "customer",
            _ => "supplier",
        }
    }
    fn is_document(self) -> bool {
        matches!(self, Self::CustomerInvoices | Self::SupplierBills)
    }
    fn is_payment(self) -> bool {
        matches!(self, Self::CustomerPayments | Self::SupplierPayments)
    }
    fn columns(self) -> &'static [&'static str] {
        if self.is_document() {
            &[
                "reference",
                "party_id",
                "document_date",
                "due_date",
                "total",
                "counter_account",
                "description",
            ]
        } else if self.is_payment() {
            &[
                "reference",
                "document_reference",
                "party_id",
                "payment_date",
                "amount",
                "cash_account_code",
            ]
        } else {
            &[
                "reference",
                "posting_date",
                "description",
                "debit_account",
                "credit_account",
                "amount",
            ]
        }
    }
    fn target(self) -> &'static str {
        if self.side() == "customer" {
            "historical_receivable"
        } else {
            "historical_payable"
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    csv: String,
    #[serde(default)]
    delimiter: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlanRow {
    row: usize,
    data: BTreeMap<String, String>,
    errors: Vec<String>,
    effect: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommitInput {
    confirmed: bool,
    idempotency_key: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}
fn clean(value: &str, max: usize, label: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "{label} wajib diisi, maksimal {max} karakter tanpa karakter kontrol."
        )));
    }
    Ok(value.to_owned())
}
fn positive(value: &str) -> AppResult<Decimal> {
    let value = fiscal::line_value(value)?;
    if value == Decimal::ZERO {
        return Err(AppError::bad_request("Nominal harus lebih dari nol."));
    }
    Ok(value)
}
fn field<'a>(row: &'a PlanRow, name: &str) -> &'a str {
    row.data.get(name).map(String::as_str).unwrap_or("")
}
fn uuid(value: &str) -> AppResult<Uuid> {
    value
        .parse()
        .map_err(|_| AppError::bad_request("party_id harus UUID pelanggan/supplier yang sesuai."))
}
fn date(row: &PlanRow, kind: Kind) -> AppResult<NaiveDate> {
    fiscal::calendar(field(
        row,
        if kind.is_document() {
            "document_date"
        } else if kind.is_payment() {
            "payment_date"
        } else {
            "posting_date"
        },
    ))
}

async fn counter(tx: &mut Transaction<'_, Postgres>, code: &str, side: &str) -> AppResult<()> {
    let row=sqlx::query("SELECT kind,system_role FROM accounts WHERE code=$1 AND code NOT IN ('1100','1200','1300') AND NOT EXISTS(SELECT 1 FROM treasury_accounts t WHERE t.account_code=accounts.code)").bind(code).fetch_optional(&mut **tx).await?.ok_or_else(||AppError::bad_request("Akun lawan tidak tersedia atau merupakan kas/bank/kontrol."))?;
    let kind: String = row.get("kind");
    if row.get::<Option<String>, _>("system_role").is_some()
        || !if side == "customer" {
            matches!(kind.as_str(), "revenue" | "equity")
        } else {
            matches!(kind.as_str(), "expense" | "asset" | "equity")
        }
    {
        return Err(AppError::bad_request(
            "Akun lawan harus pendapatan/ekuitas untuk invoice atau beban/aset nonkas/ekuitas untuk tagihan.",
        ));
    }
    Ok(())
}
async fn party(tx: &mut Transaction<'_, Postgres>, side: &str, id: Uuid) -> AppResult<String> {
    let table = if side == "customer" {
        "customers"
    } else {
        "suppliers"
    };
    sqlx::query_scalar(&format!("SELECT name FROM {table} WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::bad_request("Pihak tidak ditemukan pada jenis transaksi ini."))
}
async fn dated_cash_balance(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
    date: NaiveDate,
    amount: Decimal,
) -> AppResult<()> {
    crate::treasury::require_balance(tx, code, amount).await?;
    let before:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(l.debit-l.credit),0) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c WHERE l.account_code=$1 AND (j.created_at AT TIME ZONE c.timezone)::date<=$2").bind(code).bind(date).fetch_one(&mut **tx).await?;
    let minimum:Option<Decimal>=sqlx::query_scalar("WITH daily AS(SELECT (j.created_at AT TIME ZONE c.timezone)::date financial_date,sum(l.debit-l.credit) value FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id CROSS JOIN company c WHERE l.account_code=$1 GROUP BY financial_date), running AS(SELECT financial_date,sum(value) OVER(ORDER BY financial_date) value FROM daily) SELECT min(value) FROM running WHERE financial_date>=$2").bind(code).bind(date).fetch_one(&mut **tx).await?;
    let minimum = minimum.unwrap_or(before);
    if before < amount || minimum < amount {
        return Err(AppError::conflict(
            "Saldo kas/bank pada tanggal historis atau sesudahnya tidak cukup; impor dana masuk lebih dahulu.",
        ));
    }
    Ok(())
}
async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    row: &PlanRow,
    reserved: &mut BTreeMap<Uuid, Decimal>,
    cash_reserved: &mut BTreeMap<String, Decimal>,
) -> AppResult<Value> {
    let reference = clean(field(row, "reference"), 100, "Referensi")?;
    let posting = date(row, kind)?;
    fiscal::check_date(tx, posting).await?;
    if kind.is_document() {
        let party_id = uuid(field(row, "party_id"))?;
        let name = party(tx, kind.side(), party_id).await?;
        let total = positive(field(row, "total"))?;
        let due = fiscal::calendar(field(row, "due_date"))?;
        if due < posting {
            return Err(AppError::bad_request(
                "Jatuh tempo sebelum tanggal dokumen.",
            ));
        }
        clean(field(row, "description"), 500, "Keterangan")?;
        counter(tx, field(row, "counter_account"), kind.side()).await?;
        let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM historical_documents WHERE side=$1 AND lower(reference)=lower($2))").bind(kind.side()).bind(reference).fetch_one(&mut **tx).await?;
        if duplicate {
            return Err(AppError::conflict(
                "Referensi dokumen historis sudah pernah diimpor, termasuk yang dibalik.",
            ));
        }
        let already_recorded:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM opening_party_items WHERE party_id=$1 AND kind=CASE WHEN $2='customer' THEN 'receivable' ELSE 'payable' END AND lower(reference)=lower($3)) OR EXISTS(SELECT 1 FROM invoices WHERE customer_id=$1 AND $2='customer' AND lower(number)=lower($3)) OR EXISTS(SELECT 1 FROM purchase_bills WHERE supplier_id=$1 AND $2='supplier' AND lower(supplier_invoice_number)=lower($3))").bind(party_id).bind(kind.side()).bind(field(row,"reference")).fetch_one(&mut **tx).await?;
        if already_recorded {
            return Err(AppError::conflict(
                "Referensi ini sudah tercatat pada saldo awal atau dokumen operasional; impor akan menggandakan saldo.",
            ));
        }
        Ok(
            json!({"party_name":name,"debit":if kind.side()=="customer"{"1200"}else{field(row,"counter_account")},"credit":if kind.side()=="customer"{field(row,"counter_account").to_owned()}else{sqlx::query_scalar::<_,String>("SELECT code FROM accounts WHERE system_role='purchase_payable'").fetch_one(&mut **tx).await?},"amount":total.to_string(),"posting_date":posting,"stock_change":"0"}),
        )
    } else if kind.is_payment() {
        let party_id = uuid(field(row, "party_id"))?;
        party(tx, kind.side(), party_id).await?;
        let amount = positive(field(row, "amount"))?;
        let doc=sqlx::query("SELECT d.id,d.total,d.document_date,historical_document_paid(d.id) paid FROM historical_documents d WHERE side=$1 AND lower(reference)=lower($2) AND party_id=$3 AND NOT EXISTS(SELECT 1 FROM historical_reversals r WHERE r.document_id=d.id)").bind(kind.side()).bind(field(row,"document_reference")).bind(party_id).fetch_optional(&mut **tx).await?.ok_or_else(||AppError::bad_request("Dokumen historis aktif dengan referensi dan pihak yang sama tidak ditemukan."))?;
        if posting < doc.get::<NaiveDate, _>("document_date") {
            return Err(AppError::bad_request(
                "Pembayaran tidak boleh mendahului dokumen historis.",
            ));
        }
        let id: Uuid = doc.get("id");
        let prior = *reserved.get(&id).unwrap_or(&Decimal::ZERO);
        if amount + prior > doc.get::<Decimal, _>("total") - doc.get::<Decimal, _>("paid") {
            return Err(AppError::conflict(
                "Pembayaran melebihi saldo dokumen, termasuk baris lain pada batch ini.",
            ));
        }
        let code = crate::treasury::cash_account(tx, Some(field(row, "cash_account_code"))).await?;
        if kind.side() == "supplier" {
            let cash_prior = *cash_reserved.get(&code).unwrap_or(&Decimal::ZERO);
            dated_cash_balance(tx, &code, posting, amount + cash_prior).await?;
            cash_reserved.insert(code.clone(), cash_prior + amount);
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM historical_payments WHERE lower(reference)=lower($1))",
        )
        .bind(reference)
        .fetch_one(&mut **tx)
        .await?;
        if duplicate {
            return Err(AppError::conflict(
                "Referensi pembayaran historis sudah dipakai.",
            ));
        }
        reserved.insert(id, prior + amount);
        let control = if kind.side() == "customer" {
            "1200".to_owned()
        } else {
            sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
                .fetch_one(&mut **tx)
                .await?
        };
        Ok(
            json!({"document_id":id,"document_version":sqlx::query_scalar::<_,i64>("SELECT version FROM party_target_versions WHERE target_type=$1 AND target_id=$2").bind(kind.target()).bind(id).fetch_one(&mut **tx).await?,"debit":if kind.side()=="customer"{code.clone()}else{control.clone()},"credit":if kind.side()=="customer"{control}else{code},"amount":amount.to_string(),"posting_date":posting,"stock_change":"0"}),
        )
    } else {
        clean(field(row, "description"), 500, "Keterangan")?;
        let amount = positive(field(row, "amount"))?;
        for name in ["debit_account", "credit_account"] {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE code=$1 AND code NOT IN ('1200','1300') AND system_role IS NULL)").bind(field(row,name)).fetch_one(&mut **tx).await?;
            if !exists {
                return Err(AppError::bad_request(
                    "Akun jurnal tidak ditemukan atau akun kontrol.",
                ));
            }
        }
        if field(row, "debit_account") == field(row, "credit_account") {
            return Err(AppError::bad_request(
                "Akun debit dan kredit harus berbeda.",
            ));
        }
        let is_cash: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM treasury_accounts WHERE account_code=$1)",
        )
        .bind(field(row, "credit_account"))
        .fetch_one(&mut **tx)
        .await?;
        if is_cash {
            let code = field(row, "credit_account");
            let prior = *cash_reserved.get(code).unwrap_or(&Decimal::ZERO);
            dated_cash_balance(tx, code, posting, amount + prior).await?;
            cash_reserved.insert(code.to_owned(), prior + amount);
        }
        let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM historical_import_journals WHERE lower(reference)=lower($1))").bind(reference).fetch_one(&mut **tx).await?;
        if duplicate {
            return Err(AppError::conflict("Referensi jurnal impor sudah dipakai."));
        }
        Ok(
            json!({"debit":field(row,"debit_account"),"credit":field(row,"credit_account"),"amount":amount.to_string(),"posting_date":posting,"stock_change":"0"}),
        )
    }
}
async fn plan(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    csv: &str,
    delimiter: char,
) -> AppResult<Value> {
    if csv.len() > 256 * 1024 {
        return Err(AppError::bad_request("CSV maksimal 256 KiB."));
    }
    if !matches!(delimiter, ',' | ';') {
        return Err(AppError::bad_request("Pemisah harus koma atau titik koma."));
    }
    let records =
        crate::imports::parse_csv(csv, delimiter).map_err(|e| AppError::bad_request(e.message))?;
    if records.len() < 2 || records.len() > 1001 {
        return Err(AppError::bad_request(
            "CSV memerlukan header dan 1–1.000 baris data.",
        ));
    }
    let header: &Vec<String> = &records[0].fields;
    if header.iter().map(|v| v.trim()).collect::<Vec<_>>() != kind.columns() {
        return Err(AppError::bad_request(format!(
            "Header harus persis: {}",
            kind.columns().join(",")
        )));
    }
    let mut rows = Vec::new();
    let mut references = BTreeSet::new();
    let mut reserved = BTreeMap::new();
    let mut cash_reserved = BTreeMap::new();
    for record in records.iter().skip(1) {
        let mut row = PlanRow {
            row: record.line,
            data: BTreeMap::new(),
            errors: Vec::new(),
            effect: Value::Null,
        };
        if record.fields.len() != header.len() {
            row.errors
                .push("Jumlah kolom tidak cocok dengan header.".into());
        }
        for (i, column) in header.iter().enumerate() {
            let value = record.fields.get(i).map(|v| v.trim()).unwrap_or("");
            row.data
                .insert(column.trim().to_owned(), value.replace('\0', "\\0"));
        }
        if record.fields.iter().any(|v| v.contains('\0')) {
            row.errors.push("CSV mengandung karakter NUL.".into());
        }
        if !references.insert(field(&row, "reference").to_lowercase()) {
            row.errors.push("Referensi ganda dalam CSV.".into());
        }
        if row.errors.is_empty() {
            match validate(tx, kind, &row, &mut reserved, &mut cash_reserved).await {
                Ok(effect) => row.effect = effect,
                Err(error) => row.errors.push(error.message),
            }
        }
        rows.push(row);
    }
    let errors: usize = rows.iter().map(|r| r.errors.len()).sum();
    Ok(
        json!({"rows":rows,"row_count":rows.len(),"error_count":errors,"can_commit":errors==0,"period_version":sqlx::query_scalar::<_,i64>("SELECT version FROM accounting_period WHERE singleton").fetch_one(&mut **tx).await?,"warning":"Hanya riwayat finansial: stok, HPP, pesanan, ongkir, dan pajak tidak dibuat ulang. Jangan mengimpor dokumen yang sudah tercakup pada saldo awal."}),
    )
}
async fn template(Path(kind): Path<String>, user: AuthUser) -> AppResult<String> {
    user.require(&["finance"])?;
    Ok(format!("{}\n", Kind::parse(&kind)?.columns().join(",")))
}
async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
    Json(body): Json<PreviewInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let kind = Kind::parse(&name)?;
    let delim = body.delimiter.as_deref().unwrap_or(",");
    let delimiter = match delim {
        "," => ',',
        ";" => ';',
        _ => return Err(AppError::bad_request("Pemisah CSV tidak valid.")),
    };
    let mut tx = state.db.begin().await?;
    let plan = plan(&mut tx, kind, &body.csv, delimiter).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO transaction_import_batches(id,kind,csv,delimiter,preview,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(name).bind(body.csv).bind(delim).bind(&plan).bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"preview":plan,"committed_at":null})))
}
async fn history(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(item ORDER BY created_at DESC,id),'[]'::jsonb) FROM(SELECT id,created_at,jsonb_build_object('id',id,'kind',kind,'created_at',created_at,'committed_at',committed_at,'row_count',preview->'row_count','error_count',preview->'error_count') item FROM transaction_import_batches WHERE created_by=$1 OR $2 ORDER BY created_at DESC,id LIMIT 200)x").bind(user.id).bind(user.role=="admin").fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
async fn detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let value:Value=sqlx::query_scalar("SELECT to_jsonb(b)-'csv' FROM transaction_import_batches b WHERE id=$1 AND(created_by=$2 OR $3)").bind(id).bind(user.id).bind(user.role=="admin").fetch_optional(&state.db).await?.ok_or_else(AppError::not_found)?;
    Ok(Json(value))
}

struct SourceJournal<'a> {
    date: NaiveDate,
    kind: &'a str,
    source: Uuid,
    description: &'a str,
    debit: &'a str,
    credit: &'a str,
    amount: Decimal,
}
async fn source_journal(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    posting: SourceJournal<'_>,
) -> AppResult<(Uuid, chrono::DateTime<chrono::Utc>)> {
    let SourceJournal {
        date,
        kind,
        source,
        description,
        debit,
        credit,
        amount,
    } = posting;
    fiscal::check_date(tx, date).await?;
    let at = fiscal::timestamp(tx, date).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(description).bind(kind).bind(source).bind(user.id).bind(at).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,0),($5,$2,$6,0,$4)").bind(Uuid::new_v4()).bind(id).bind(debit).bind(amount).bind(Uuid::new_v4()).bind(credit).execute(&mut **tx).await?;
    Ok((id, at))
}
async fn commit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CommitInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if !body.confirmed {
        return Err(AppError::bad_request(
            "Konfirmasi bahwa CSV tidak menggandakan saldo awal dan hanya mengimpor riwayat finansial.",
        ));
    }
    let request = payload(
        "transaction_import.commit",
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await?;
    let batch = sqlx::query(
        "SELECT * FROM transaction_import_batches WHERE id=$1 AND(created_by=$2 OR $3) FOR UPDATE",
    )
    .bind(id)
    .bind(user.id)
    .bind(user.role == "admin")
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(AppError::not_found)?;
    if batch
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("committed_at")
        .is_some()
    {
        return Err(AppError::conflict(
            "Batch ini sudah diposting; buka hasil impor yang tersimpan.",
        ));
    }
    let kind = Kind::parse(batch.get("kind"))?;
    let csv: String = batch.get("csv");
    let delimiter: String = batch.get("delimiter");
    let saved: Value = batch.get("preview");
    let latest = plan(&mut tx, kind, &csv, delimiter.chars().next().unwrap()).await?;
    if latest["can_commit"] != true || latest != saved {
        return Err(AppError::conflict(
            "Validasi, saldo, dokumen, atau periode berubah sejak preview. Buat preview baru sebelum posting.",
        ));
    }
    let rows: Vec<PlanRow> =
        serde_json::from_value(latest["rows"].clone()).map_err(|e| AppError::internal(e.into()))?;
    let mut results = Vec::new();
    for row in rows {
        let source = Uuid::new_v4();
        let posting = date(&row, kind)?;
        let amount = positive(row.effect["amount"].as_str().unwrap())?;
        let reference = field(&row, "reference");
        if kind.is_document() {
            let description = format!(
                "Impor finansial {}: {}",
                reference,
                field(&row, "description")
            );
            let (journal, at) = source_journal(
                &mut tx,
                &user,
                SourceJournal {
                    date: posting,
                    kind: "historical_document",
                    source,
                    description: &description,
                    debit: row.effect["debit"].as_str().unwrap(),
                    credit: row.effect["credit"].as_str().unwrap(),
                    amount,
                },
            )
            .await?;
            sqlx::query("INSERT INTO historical_documents(id,side,reference,party_id,total,counter_account,journal_id,document_date,due_date,description,created_at,created_by,batch_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(source).bind(kind.side()).bind(reference).bind(uuid(field(&row,"party_id"))?).bind(amount).bind(field(&row,"counter_account")).bind(journal).bind(posting).bind(fiscal::calendar(field(&row,"due_date"))?).bind(field(&row,"description")).bind(at).bind(user.id).bind(id).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO party_target_versions(target_type,target_id) VALUES($1,$2)")
                .bind(kind.target())
                .bind(source)
                .execute(&mut *tx)
                .await?;
            results.push(json!({"id":source,"reference":reference,"kind":"historical_document","journal_id":journal,"url":format!("?historical={source}#transaction-imports")}));
        } else if kind.is_payment() {
            let document: Uuid = row.effect["document_id"].as_str().unwrap().parse().unwrap();
            let (journal, at) = source_journal(
                &mut tx,
                &user,
                SourceJournal {
                    date: posting,
                    kind: "historical_payment",
                    source,
                    description: &format!("Pembayaran historis {reference}"),
                    debit: row.effect["debit"].as_str().unwrap(),
                    credit: row.effect["credit"].as_str().unwrap(),
                    amount,
                },
            )
            .await?;
            sqlx::query("INSERT INTO historical_payments(id,document_id,reference,amount,cash_account_code,journal_id,payment_date,created_at,created_by,batch_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(source).bind(document).bind(reference).bind(amount).bind(field(&row,"cash_account_code")).bind(journal).bind(posting).bind(at).bind(user.id).bind(id).execute(&mut *tx).await?;
            sqlx::query("UPDATE party_target_versions SET version=version+1 WHERE target_type=$1 AND target_id=$2").bind(kind.target()).bind(document).execute(&mut *tx).await?;
            results.push(json!({"id":source,"document_id":document,"reference":reference,"kind":"historical_payment","journal_id":journal,"url":format!("?historical={document}#transaction-imports")}));
        } else {
            let lines = vec![
                fiscal::DatedLine {
                    account_code: field(&row, "debit_account").to_owned(),
                    debit: amount.to_string(),
                    credit: "0".into(),
                },
                fiscal::DatedLine {
                    account_code: field(&row, "credit_account").to_owned(),
                    debit: "0".into(),
                    credit: amount.to_string(),
                },
            ];
            let journal = fiscal::insert_dated_journal(
                &mut tx,
                &user,
                posting,
                &format!("Impor jurnal {reference}: {}", field(&row, "description")),
                &lines,
            )
            .await?;
            sqlx::query("INSERT INTO historical_import_journals(id,reference,posting_date,journal_id,batch_id,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(source).bind(reference).bind(posting).bind(journal).bind(id).bind(user.id).execute(&mut *tx).await?;
            results.push(
                json!({"id":source,"reference":reference,"kind":"journal","journal_id":journal}),
            );
        }
    }
    let result = json!({"id":id,"items":results,"count":results.len(),"stock_change":"0"});
    sqlx::query(
        "UPDATE transaction_import_batches SET committed_at=now(),committed_result=$2 WHERE id=$1",
    )
    .bind(id)
    .bind(&result)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        &user,
        "transaction_import.committed",
        "transaction_import",
        id,
        json!({"result":result,"financial_only":true}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

const DOCUMENT_SQL: &str = r#"SELECT to_jsonb(d)-'customer_id'-'supplier_id'||jsonb_build_object('number',d.reference,'total',d.total::text,'party_name',CASE WHEN d.side='customer' THEN c.name ELSE s.name END,'paid',historical_document_paid(d.id)::text,'outstanding',CASE WHEN r.id IS NULL THEN (d.total-historical_document_paid(d.id))::text ELSE '0' END,'version',v.version,'reversal',to_jsonb(r),'can_reverse',r.id IS NULL AND historical_document_paid(d.id)=0 AND cfg.period_open,'payments',COALESCE((SELECT jsonb_agg(to_jsonb(p)||jsonb_build_object('amount',p.amount::text,'reversal',to_jsonb(pr),'can_reverse',pr.id IS NULL AND cfg.period_open) ORDER BY p.created_at,p.id) FROM historical_payments p LEFT JOIN historical_reversals pr ON pr.payment_id=p.id WHERE p.document_id=d.id),'[]'::jsonb),'fund_allocations',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'fund_id',a.fund_id,'fund_number',(SELECT number FROM party_funds f WHERE f.id=a.fund_id),'reversed',party_reversal_document('party_allocation',a.id) IS NOT NULL,'amount',a.amount::text,'journal_id',a.journal_id,'created_at',a.created_at,'reversal',party_reversal_document('party_allocation',a.id)) ORDER BY a.created_at,a.id) FROM party_allocations a WHERE a.target_id=d.id AND a.target_type IN ('historical_receivable','historical_payable')),'[]'::jsonb)) document FROM historical_documents d LEFT JOIN customers c ON c.id=d.customer_id LEFT JOIN suppliers s ON s.id=d.supplier_id LEFT JOIN historical_reversals r ON r.document_id=d.id JOIN party_target_versions v ON v.target_id=d.id AND v.target_type=CASE WHEN d.side='customer' THEN 'historical_receivable' ELSE 'historical_payable' END CROSS JOIN (SELECT closed_through IS NULL OR (now() AT TIME ZONE timezone)::date>closed_through period_open FROM company CROSS JOIN accounting_period WHERE accounting_period.singleton)cfg"#;
pub(crate) async fn read_document(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<Value> {
    sqlx::query_scalar(&format!(
        "SELECT document FROM ({DOCUMENT_SQL} WHERE d.id=$1)x"
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(AppError::not_found)
}
async fn document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mut tx = state.db.begin().await?;
    let value = read_document(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(value))
}
async fn documents(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let items:Value=sqlx::query_scalar(&format!("SELECT COALESCE(jsonb_agg(document ORDER BY document->>'created_at' DESC,document->>'id'),'[]'::jsonb) FROM({DOCUMENT_SQL} ORDER BY d.created_at DESC,d.id LIMIT 500)x")).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
async fn reverse(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    body: ReverseInput,
    payment: bool,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = clean(&body.reason, 1000, "Alasan")?;
    if reason.chars().count() < 3 {
        return Err(AppError::bad_request("Alasan minimal tiga karakter."));
    }
    let request = payload(
        "historical.reverse",
        &user,
        &json!({"id":id,"payment":payment,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    crate::corrections::check_open(&mut tx).await?;
    let document_id = if payment {
        sqlx::query_scalar::<_, Uuid>("SELECT document_id FROM historical_payments WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?
    } else {
        id
    };
    let d=sqlx::query("SELECT d.*,v.version FROM historical_documents d JOIN party_target_versions v ON v.target_id=d.id AND v.target_type=CASE WHEN d.side='customer' THEN 'historical_receivable' ELSE 'historical_payable' END WHERE d.id=$1 FOR UPDATE OF d,v").bind(document_id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    if body.version != d.get::<i64, _>("version") {
        return Err(AppError::conflict(
            "Versi dokumen historis berubah; muat ulang.",
        ));
    }
    let reversed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM historical_reversals WHERE document_id=$1 OR payment_id=$2)",
    )
    .bind(document_id)
    .bind(if payment { Some(id) } else { None })
    .fetch_one(&mut *tx)
    .await?;
    if reversed {
        return Err(AppError::conflict("Dokumen atau pembayaran sudah dibalik."));
    }
    let journal = if payment {
        let p = sqlx::query("SELECT * FROM historical_payments WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if d.get::<String, _>("side") == "customer" {
            crate::treasury::require_balance(&mut tx, p.get("cash_account_code"), p.get("amount"))
                .await?;
        }
        p.get::<Uuid, _>("journal_id")
    } else {
        let paid: Decimal = sqlx::query_scalar("SELECT historical_document_paid($1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if paid != Decimal::ZERO {
            return Err(AppError::conflict(
                "Balikkan pembayaran dan alokasi aktif sebelum membalik dokumen historis.",
            ));
        }
        d.get::<Uuid, _>("journal_id")
    };
    let reversal = Uuid::new_v4();
    let new_journal = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'historical_reversal',$3,$4)").bind(new_journal).bind(format!("Pembalikan historis {}: {reason}",d.get::<String,_>("reference"))).bind(reversal).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$2,account_code,credit,debit FROM journal_lines WHERE journal_id=$1").bind(journal).bind(new_journal).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO historical_reversals(id,document_id,payment_id,reason,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(reversal).bind(if payment{None}else{Some(id)}).bind(if payment{Some(id)}else{None}).bind(reason).bind(new_journal).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE party_target_versions SET version=version+1 WHERE target_id=$1 AND target_type IN ('historical_receivable','historical_payable')").bind(document_id).execute(&mut *tx).await?;
    let result = read_document(&mut tx, document_id).await?;
    audit(
        &mut tx,
        &user,
        "historical.reversed",
        "historical_document",
        document_id,
        json!({"payment":payment,"source_id":id,"journal_id":new_journal}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn reverse_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    reverse(state, user, id, body, false).await
}
async fn reverse_payment(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    reverse(state, user, id, body, true).await
}
