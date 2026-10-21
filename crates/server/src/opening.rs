use std::{collections::BTreeSet, str::FromStr};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post},
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, parse, payload, remember},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/accounting/opening",
            get(read)
                .put(save)
                .layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/accounting/opening/parties/preview",
            post(preview_parties).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/accounting/opening/parties/template", get(party_template))
        .route("/accounting/opening/post", post(post_opening))
        .route("/accounting/opening/reverse", post(reverse_opening))
        .route("/opening-items/{id}", get(party_item_detail))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemInput {
    product_id: Uuid,
    quantity: String,
    unit_cost: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stock_context: Option<crate::inventory_extensions::StockContext>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LineInput {
    account_code: String,
    debit: String,
    credit: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DraftInput {
    version: i32,
    description: String,
    items: Vec<ItemInput>,
    lines: Vec<LineInput>,
    #[serde(default)]
    parties: Vec<PartyInput>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PartyInput {
    kind: String,
    party_id: Uuid,
    reference: String,
    document_date: String,
    due_date: Option<String>,
    amount: String,
}
#[derive(Clone, Deserialize, Serialize)]
struct Party {
    kind: String,
    party_id: Uuid,
    party_name: String,
    reference: String,
    document_date: String,
    due_date: Option<String>,
    amount: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PostInput {
    version: i32,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

#[derive(Clone, Deserialize, Serialize)]
struct Item {
    product_id: Uuid,
    sku: String,
    name: String,
    unit: String,
    quantity: String,
    unit_cost: String,
    value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stock_context: Option<crate::inventory_extensions::StockContext>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Line {
    account_code: String,
    account_name: String,
    debit: String,
    credit: String,
    #[serde(default)]
    generated: bool,
}

struct Draft {
    items: Vec<Item>,
    lines: Vec<Line>,
    inventory: Decimal,
    debit: Decimal,
    credit: Decimal,
    parties: Vec<Party>,
}

fn number(value: &str) -> AppResult<Decimal> {
    Decimal::from_str(value).map_err(|_| AppError::bad_request("Invalid stored decimal value"))
}

fn nonnegative(value: &str) -> AppResult<Decimal> {
    let amount = number(value)?;
    if amount < Decimal::ZERO
        || amount > Decimal::from(1_000_000_000_000u64)
        || amount.normalize().scale() > 2
    {
        return Err(AppError::bad_request(
            "Opening debit and credit must be nonnegative, at most 1000000000000, with at most 2 decimal places",
        ));
    }
    Ok(amount)
}

fn too_large(debit: Decimal, credit: Decimal) -> bool {
    debit > Decimal::from(1_000_000_000_000u64) || credit > Decimal::from(1_000_000_000_000u64)
}

fn opening_date(value: &str) -> AppResult<NaiveDate> {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        || value.starts_with("0000")
    {
        return Err(AppError::bad_request(
            "Tanggal harus YYYY-MM-DD tahun 0001–9999.",
        ));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Tanggal kalender tidak valid."))
}
async fn validate_parties(
    tx: &mut Transaction<'_, Postgres>,
    inputs: &[PartyInput],
) -> AppResult<Vec<Party>> {
    if inputs.len() > 1000 {
        return Err(AppError::bad_request(
            "Maksimal 1.000 rincian pihak pada saldo awal.",
        ));
    }
    let today: NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date FROM company")
            .fetch_one(&mut **tx)
            .await?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut debit = Decimal::ZERO;
    let mut credit = Decimal::ZERO;
    for input in inputs {
        let customer = match input.kind.as_str() {
            "receivable" | "customer_credit" => true,
            "payable" | "supplier_credit" => false,
            _ => return Err(AppError::bad_request("Jenis saldo pihak tidak didukung.")),
        };
        let reference = crate::supplier_bills::clean_text(&input.reference, 1, 200, false)?;
        if !seen.insert((input.kind.clone(), input.party_id, reference.to_uppercase())) {
            return Err(AppError::bad_request(
                "Referensi saldo awal pihak yang sama muncul lebih dari sekali.",
            ));
        }
        let date = opening_date(&input.document_date)?;
        let due = input
            .due_date
            .as_deref()
            .filter(|v| !v.is_empty())
            .map(opening_date)
            .transpose()?;
        if date > today || due.is_some_and(|due| due < date) {
            return Err(AppError::bad_request(
                "Tanggal dokumen tidak boleh di masa depan dan jatuh tempo tidak sebelum dokumen.",
            ));
        }
        let amount = domain::payment_amount(&input.amount).map_err(AppError::bad_request)?;
        if matches!(input.kind.as_str(), "receivable" | "supplier_credit") {
            debit += amount;
        } else {
            credit += amount;
        }
        if too_large(debit, credit) {
            return Err(AppError::bad_request(
                "Total saldo pihak per sisi maksimal Rp1 triliun.",
            ));
        }
        let query = if customer {
            "SELECT name FROM customers WHERE id=$1 FOR KEY SHARE"
        } else {
            "SELECT name FROM suppliers WHERE id=$1 FOR KEY SHARE"
        };
        let name: String = sqlx::query_scalar(query)
            .bind(input.party_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| {
                AppError::bad_request("Pelanggan atau supplier saldo awal tidak ditemukan.")
            })?;
        result.push(Party {
            kind: input.kind.clone(),
            party_id: input.party_id,
            party_name: name,
            reference,
            document_date: date.to_string(),
            due_date: due.map(|v| v.to_string()),
            amount: amount.to_string(),
        });
    }
    Ok(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartyPreview {
    csv: String,
    delimiter: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartyTemplateFilter {
    delimiter: Option<String>,
}

async fn party_item_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result:Value=sqlx::query_scalar(r#"SELECT to_jsonb(i)-'customer_id'-'supplier_id'||jsonb_build_object(
      'party_name',COALESCE(c.name,s.name),'side',CASE WHEN i.kind IN ('receivable','customer_credit') THEN 'customer' ELSE 'supplier' END,
      'target_type',CASE WHEN i.kind='receivable' THEN 'opening_receivable' WHEN i.kind='payable' THEN 'opening_payable' ELSE NULL END,
      'amount',i.amount::text,'paid',COALESCE(a.paid,0)::text,
      'outstanding',(CASE WHEN r.id IS NOT NULL OR i.kind IN ('customer_credit','supplier_credit') THEN 0 ELSE i.amount-COALESCE(a.paid,0) END)::text,
      'version',COALESCE(v.version,1),'journal_id',p.journal_id,'document_id',p.document_id,'posted_at',p.posted_at,
      'status',CASE WHEN r.id IS NOT NULL THEN 'void' WHEN i.kind IN ('customer_credit','supplier_credit') THEN 'credit' WHEN COALESCE(a.paid,0)=i.amount THEN 'paid' WHEN COALESCE(a.paid,0)>0 THEN 'partial' ELSE 'unpaid' END,
      'credit_fund_id',(SELECT id FROM party_funds f WHERE f.source_type='opening_item' AND f.source_id=i.id),
      'allocations',COALESCE((SELECT jsonb_agg(to_jsonb(x)||jsonb_build_object('amount',x.amount::text,'reversal',party_reversal_document('party_allocation',x.id)) ORDER BY x.created_at,x.id) FROM party_allocations x WHERE x.target_id=i.id AND x.target_type=CASE WHEN i.kind='receivable' THEN 'opening_receivable' ELSE 'opening_payable' END),'[]'::jsonb))
      FROM opening_party_items i JOIN opening_postings p ON p.id=i.posting_id LEFT JOIN customers c ON c.id=i.customer_id LEFT JOIN suppliers s ON s.id=i.supplier_id
      LEFT JOIN posting_reversals r ON r.source_type='opening' AND r.source_id=p.id
      LEFT JOIN party_target_versions v ON v.target_id=i.id AND v.target_type=CASE WHEN i.kind='receivable' THEN 'opening_receivable' ELSE 'opening_payable' END
      LEFT JOIN LATERAL(SELECT sum(x.amount) paid FROM party_allocations x WHERE x.target_id=i.id AND x.target_type=CASE WHEN i.kind='receivable' THEN 'opening_receivable' ELSE 'opening_payable' END AND NOT EXISTS(SELECT 1 FROM posting_reversals pr WHERE pr.source_type='party_allocation' AND pr.source_id=x.id))a ON true
      WHERE i.id=$1"#).bind(id).fetch_optional(&state.db).await?.ok_or_else(AppError::not_found)?;
    Ok(Json(result))
}

// Preserve physical line numbers while accepting quoted delimiters/newlines.
fn party_csv(csv: &str, delimiter: char) -> AppResult<Vec<(usize, Vec<String>)>> {
    let mut chars = csv
        .strip_prefix('\u{feff}')
        .unwrap_or(csv)
        .chars()
        .peekable();
    let mut rows = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut started = false;
    let mut line = 1;
    let mut start = 1;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => {
                    quoted = false;
                    closed = true;
                }
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    field.push('\n');
                    line += 1;
                }
                _ => field.push(c),
            }
            continue;
        }
        if c == delimiter {
            fields.push(std::mem::take(&mut field));
            closed = false;
            started = true;
        } else if matches!(c, '\r' | '\n') {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            fields.push(std::mem::take(&mut field));
            if started || fields.len() > 1 || !fields[0].is_empty() {
                rows.push((start, std::mem::take(&mut fields)));
            } else {
                fields.clear();
            }
            line += 1;
            start = line;
            closed = false;
            started = false;
        } else if c == '"' && field.is_empty() && !closed {
            quoted = true;
            started = true;
        } else {
            if closed || c == '"' {
                return Err(AppError::bad_request(format!(
                    "Baris {line}: tanda kutip CSV tidak valid."
                )));
            }
            field.push(c);
            started = true;
        }
        if rows.len() > 1001 {
            return Err(AppError::bad_request("Maksimal 1.000 baris CSV."));
        }
    }
    if quoted {
        return Err(AppError::bad_request(
            "Kolom CSV bertanda kutip belum ditutup.",
        ));
    }
    if started || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        rows.push((start, fields));
    }
    if rows.len() > 1001 {
        return Err(AppError::bad_request("Maksimal 1.000 baris CSV."));
    }
    Ok(rows)
}
async fn party_template(
    user: AuthUser,
    Query(filter): Query<PartyTemplateFilter>,
) -> AppResult<(axum::http::HeaderMap, String)> {
    user.require(&["finance"])?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=\"haruflow-saldo-pihak.csv\""),
    );
    let csv = "\u{feff}kind,party_id,party,reference,document_date,due_date,debit,credit\r\nreceivable,,Nama pelanggan,INV-LAMA-001,2026-01-01,2026-02-01,2000000,0\r\npayable,,KODE-SUPPLIER,BILL-LAMA-001,2026-01-01,,0,3000000\r\n";
    let csv = match filter.delimiter.as_deref().unwrap_or("comma") {
        "comma" => csv.to_owned(),
        "semicolon" => csv.replace(',', ";"),
        _ => return Err(AppError::bad_request("Pemisah harus comma atau semicolon.")),
    };
    Ok((headers, csv))
}
async fn preview_parties(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PartyPreview>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if body.csv.len() > 256 * 1024 {
        return Err(AppError::bad_request("CSV maksimal 256 KiB."));
    }
    let delimiter = match body.delimiter.as_str() {
        "comma" => ',',
        "semicolon" => ';',
        _ => return Err(AppError::bad_request("Pemisah harus comma atau semicolon.")),
    };
    let records = match party_csv(&body.csv, delimiter) {
        Ok(rows) => rows,
        Err(error) => {
            return Ok(Json(
                json!({"parties":[],"valid_count":0,"errors":[{"row":1,"column":"csv","message":error.message}]}),
            ));
        }
    };
    let mut errors = Vec::<Value>::new();
    let mut inputs = Vec::new();
    let mut tx = state.db.begin().await?;
    let columns = records
        .first()
        .map(|(_, cols)| cols.iter().map(|v| v.trim().to_owned()).collect::<Vec<_>>())
        .unwrap_or_default();
    let allowed = [
        "kind",
        "party_id",
        "party",
        "reference",
        "document_date",
        "due_date",
        "debit",
        "credit",
    ];
    let unique: BTreeSet<_> = columns.iter().collect();
    if columns.is_empty()
        || unique.len() != columns.len()
        || columns.iter().any(|c| !allowed.contains(&c.as_str()))
        || ["kind", "reference", "document_date", "debit", "credit"]
            .iter()
            .any(|required| !columns.iter().any(|c| c == required))
        || !columns.iter().any(|c| c == "party_id" || c == "party")
    {
        return Ok(Json(
            json!({"parties":[],"valid_count":0,"errors":[{"row":1,"column":"header","message":"Gunakan header template tanpa kolom ganda atau tambahan."}]}),
        ));
    }
    let mut seen = BTreeSet::new();
    for (row, fields) in records.into_iter().skip(1) {
        let attempt:AppResult<PartyInput>=async{
            if fields.len()!=columns.len(){return Err(AppError::bad_request("Jumlah kolom berbeda dari header."));}
            let value=|key:&str|columns.iter().position(|c|c==key).map(|i|fields[i].trim()).unwrap_or("");
            let kind=value("kind");let customer=match kind{"receivable"|"customer_credit"=>true,"payable"|"supplier_credit"=>false,_=>return Err(AppError::bad_request("Jenis saldo pihak tidak didukung."))};
            let debit=nonnegative(if value("debit").is_empty(){"0"}else{value("debit")})?;let credit=nonnegative(if value("credit").is_empty(){"0"}else{value("credit")})?;
            let debit_side=matches!(kind,"receivable"|"supplier_credit");
            if (debit_side&&(debit<=Decimal::ZERO||credit!=Decimal::ZERO))||(!debit_side&&(credit<=Decimal::ZERO||debit!=Decimal::ZERO)){return Err(AppError::bad_request("Isi nominal positif pada sisi yang sesuai: piutang/kredit supplier debit; utang/kredit pelanggan kredit."));}
            let party_id=if !value("party_id").is_empty(){value("party_id").parse::<Uuid>().map_err(|_|AppError::bad_request("party_id harus UUID."))?}else{
                let label=crate::supplier_bills::clean_text(value("party"),1,200,false)?;
                let query=if customer{"SELECT id FROM customers WHERE upper(btrim(name))=upper($1)"}else{"SELECT id FROM suppliers WHERE upper(btrim(code))=upper($1)"};
                let ids:Vec<Uuid>=sqlx::query_scalar(query).bind(label).fetch_all(&mut *tx).await?;
                if ids.len()!=1{return Err(AppError::bad_request("Pihak tidak ditemukan atau namanya ambigu; gunakan party_id."));}ids[0]
            };
            let input=PartyInput{kind:kind.to_owned(),party_id,reference:value("reference").to_owned(),document_date:value("document_date").to_owned(),due_date:if value("due_date").is_empty(){None}else{Some(value("due_date").to_owned())},amount:(debit+credit).to_string()};
            validate_parties(&mut tx,std::slice::from_ref(&input)).await?;
            if !seen.insert((input.kind.clone(),input.party_id,input.reference.trim().to_uppercase())){return Err(AppError::bad_request("Referensi pihak ganda dalam CSV."));}Ok(input)
        }.await;
        match attempt {
            Ok(input) => inputs.push(input),
            Err(error) => errors.push(json!({"row":row,"column":"party","message":error.message})),
        }
    }
    if inputs.is_empty() && errors.is_empty() {
        errors.push(
            json!({"row":1,"column":"csv","message":"Tambahkan setidaknya satu rincian pihak."}),
        );
    }
    let valid_count = inputs.len();
    let parties = if errors.is_empty() {
        json!(validate_parties(&mut tx, &inputs).await?)
    } else {
        json!([])
    };
    tx.commit().await?;
    Ok(Json(
        json!({"parties":parties,"errors":errors,"valid_count":valid_count}),
    ))
}

// All ordinary postings acquire the same accounting lock first. Draft saves use
// it too, so the initial-state check cannot race a receipt or a first journal.
async fn lock_accounting(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn lock_document(
    tx: &mut Transaction<'_, Postgres>,
) -> AppResult<Option<sqlx::postgres::PgRow>> {
    Ok(sqlx::query("SELECT id,status,version,description,items,lines,parties,active_posting_id FROM opening_documents WHERE company_id=(SELECT id FROM company) FOR UPDATE")
        .fetch_optional(&mut **tx).await?)
}

fn require_version(row: &sqlx::postgres::PgRow, version: i32) -> AppResult<()> {
    if row.get::<i32, _>("version") != version {
        return Err(AppError::conflict(
            "Opening balances changed; reload before continuing",
        ));
    }
    Ok(())
}

// Only this document's historical opening and exact reversal postings are
// exempt. A business posting remains a blocker even after it was reversed.
async fn business_exists(tx: &mut Transaction<'_, Postgres>) -> AppResult<bool> {
    Ok(sqlx::query_scalar(r#"SELECT
        EXISTS(SELECT 1 FROM journal_entries j WHERE NOT (
            (j.source_type='opening' AND EXISTS(SELECT 1 FROM opening_postings p WHERE p.id=j.source_id AND p.journal_id=j.id)) OR
            (j.source_type='reversal' AND EXISTS(SELECT 1 FROM posting_reversals r JOIN opening_postings p ON p.id=r.source_id
                WHERE r.source_type='opening' AND r.id=j.source_id)))) OR
        EXISTS(SELECT 1 FROM inventory_movements m WHERE NOT (
            m.kind IN ('opening','opening_reversal') AND EXISTS(SELECT 1 FROM opening_postings p WHERE p.id=m.opening_posting_id)))"#)
        .fetch_one(&mut **tx).await?)
}

async fn clean_draft(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    if business_exists(tx).await? {
        return Err(AppError::conflict(
            "Opening balances require a company without existing business journals or inventory movements",
        ));
    }
    let nonzero: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE quantity<>0 OR inventory_value<>0 OR average_cost<>0)")
        .fetch_one(&mut **tx).await?;
    if nonzero {
        return Err(AppError::conflict(
            "Opening balances require zero inventory before initialization",
        ));
    }
    Ok(())
}

async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    items: &[ItemInput],
    lines: &[LineInput],
    parties: &[PartyInput],
) -> AppResult<Draft> {
    if items.len() > 100 || lines.len() > 100 {
        return Err(AppError::bad_request(
            "Opening balances allow at most 100 stock items and 100 account lines",
        ));
    }
    let mut product_ids = BTreeSet::new();
    for item in items {
        if !product_ids.insert(item.product_id) {
            return Err(AppError::bad_request("Duplicate opening product"));
        }
    }
    // Match product-edit and order locking: sorted identifiers avoid cycles.
    let product_ids: Vec<_> = product_ids.into_iter().collect();
    sqlx::query("SELECT id FROM products WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&product_ids)
        .fetch_all(&mut **tx)
        .await?;
    let mut result = Draft {
        items: Vec::new(),
        lines: Vec::new(),
        inventory: Decimal::ZERO,
        debit: Decimal::ZERO,
        credit: Decimal::ZERO,
        parties: validate_parties(tx, parties).await?,
    };
    for item in items {
        let product = sqlx::query("SELECT sku,name,unit,kind FROM products WHERE id=$1")
            .bind(item.product_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Unknown opening product"))?;
        if product.get::<String, _>("kind") != "stock" {
            return Err(AppError::bad_request(
                "Services cannot have opening inventory",
            ));
        }
        let quantity = parse(&item.quantity, 3, "Opening quantity")?;
        let cost = parse(&item.unit_cost, 4, "Opening unit cost")?;
        let value = domain::money(quantity * cost);
        if value <= Decimal::ZERO || value > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request(
                "Rounded opening item value must be positive and at most 1000000000000",
            ));
        }
        result.inventory += value;
        result.items.push(Item {
            product_id: item.product_id,
            sku: product.get("sku"),
            name: product.get("name"),
            unit: product.get("unit"),
            quantity: quantity.to_string(),
            unit_cost: cost.to_string(),
            value: value.to_string(),
            stock_context: item.stock_context.clone(),
        });
    }
    let mut accounts = BTreeSet::new();
    for line in lines {
        if !accounts.insert(line.account_code.clone()) {
            return Err(AppError::bad_request("Duplicate opening account"));
        }
        if line.account_code == "1200" || line.account_code == "1300" {
            return Err(AppError::bad_request(
                "Opening receivables require customer documents; inventory is generated from opening stock items",
            ));
        }
        let account = sqlx::query("SELECT name,kind,system_role FROM accounts WHERE code=$1")
            .bind(&line.account_code)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Unknown opening account"))?;
        if account.get::<Option<String>, _>("system_role").is_some() {
            return Err(AppError::bad_request(
                "Opening control balances require their source document workflow",
            ));
        }
        if !matches!(
            account.get::<String, _>("kind").as_str(),
            "asset" | "liability" | "equity"
        ) {
            return Err(AppError::bad_request(
                "Opening lines require asset, liability or equity accounts",
            ));
        }
        let debit = nonnegative(&line.debit)?;
        let credit = nonnegative(&line.credit)?;
        if (debit > Decimal::ZERO) == (credit > Decimal::ZERO) {
            return Err(AppError::bad_request(
                "Each opening line must contain exactly one positive debit or credit",
            ));
        }
        result.debit += debit;
        result.credit += credit;
        result.lines.push(Line {
            account_code: line.account_code.clone(),
            account_name: account.get("name"),
            debit: debit.to_string(),
            credit: credit.to_string(),
            generated: false,
        });
    }
    if result.inventory > Decimal::ZERO {
        let name: String = sqlx::query_scalar("SELECT name FROM accounts WHERE code='1300'")
            .fetch_one(&mut **tx)
            .await?;
        result.lines.push(Line {
            account_code: "1300".into(),
            account_name: name,
            debit: result.inventory.to_string(),
            credit: "0".into(),
            generated: true,
        });
        result.debit += result.inventory;
    }
    let mut controls = std::collections::BTreeMap::<String, Decimal>::new();
    for party in &result.parties {
        let (account, sign) = match party.kind.as_str() {
            "receivable" => ("1200".to_owned(), Decimal::ONE),
            "payable" => (
                sqlx::query_scalar(
                    "SELECT code FROM accounts WHERE system_role='purchase_payable'",
                )
                .fetch_one(&mut **tx)
                .await?,
                -Decimal::ONE,
            ),
            "customer_credit" => (
                crate::settlements::party_control_account(tx, "customer").await?,
                -Decimal::ONE,
            ),
            _ => (
                crate::settlements::party_control_account(tx, "supplier").await?,
                Decimal::ONE,
            ),
        };
        *controls.entry(account).or_default() += number(&party.amount)? * sign;
    }
    for (code, signed) in controls {
        let debit = signed.max(Decimal::ZERO);
        let credit = (-signed).max(Decimal::ZERO);
        let name: String = sqlx::query_scalar("SELECT name FROM accounts WHERE code=$1")
            .bind(&code)
            .fetch_one(&mut **tx)
            .await?;
        result.lines.push(Line {
            account_code: code,
            account_name: name,
            debit: debit.to_string(),
            credit: credit.to_string(),
            generated: true,
        });
        result.debit += debit;
        result.credit += credit;
    }
    if too_large(result.debit, result.credit) {
        return Err(AppError::bad_request(
            "Opening journal totals must be at most 1000000000000",
        ));
    }
    Ok(result)
}

fn draft_inputs(
    row: &sqlx::postgres::PgRow,
) -> AppResult<(Vec<ItemInput>, Vec<LineInput>, Vec<PartyInput>)> {
    let items: Vec<Item> =
        serde_json::from_value(row.get("items")).map_err(|e| AppError::internal(e.into()))?;
    let lines: Vec<Line> =
        serde_json::from_value(row.get("lines")).map_err(|e| AppError::internal(e.into()))?;
    Ok((
        items
            .into_iter()
            .map(|i| ItemInput {
                product_id: i.product_id,
                quantity: i.quantity,
                unit_cost: i.unit_cost,
                stock_context: i.stock_context,
            })
            .collect(),
        lines
            .into_iter()
            .filter(|l| l.account_code != "1300" && !l.generated)
            .map(|l| LineInput {
                account_code: l.account_code,
                debit: l.debit,
                credit: l.credit,
            })
            .collect(),
        serde_json::from_value::<Vec<Party>>(row.get("parties"))
            .map_err(|e| AppError::internal(e.into()))?
            .into_iter()
            .map(|p| PartyInput {
                kind: p.kind,
                party_id: p.party_id,
                reference: p.reference,
                document_date: p.document_date,
                due_date: p.due_date,
                amount: p.amount,
            })
            .collect(),
    ))
}

async fn reversible(tx: &mut Transaction<'_, Postgres>, posting_id: Uuid) -> AppResult<bool> {
    let period_open: bool = sqlx::query_scalar("SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton")
        .fetch_one(&mut **tx).await?;
    if !period_open || business_exists(tx).await? {
        return Ok(false);
    }
    let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM opening_party_items i JOIN party_funds f ON f.source_type='opening_item' AND f.source_id=i.id CROSS JOIN LATERAL party_fund_balances(f.id)b WHERE i.posting_id=$1 AND (b.allocated>0 OR b.refunded>0 OR b.reserved>0))").bind(posting_id).fetch_one(&mut **tx).await?;
    if used {
        return Ok(false);
    }
    // Guard against external inventory edits too. Products not in the opening
    // must remain empty; opening products must retain exact carrying values.
    let mismatch:bool=sqlx::query_scalar(r#"SELECT EXISTS(
        SELECT 1 FROM products p LEFT JOIN
            (SELECT (i->>'product_id')::uuid product_id,(i->>'quantity')::numeric quantity,(i->>'value')::numeric value
             FROM opening_postings o CROSS JOIN LATERAL jsonb_array_elements(o.items) i WHERE o.id=$1) s ON s.product_id=p.id
        WHERE p.quantity<>COALESCE(s.quantity,0) OR p.inventory_value<>COALESCE(s.value,0)
            OR p.average_cost<>CASE WHEN s.quantity>0 THEN round(s.value/s.quantity,4) ELSE 0 END)"#)
        .bind(posting_id).fetch_one(&mut **tx).await?;
    Ok(!mismatch)
}

async fn envelope(tx: &mut Transaction<'_, Postgres>) -> AppResult<Value> {
    let row=sqlx::query("SELECT id,status,version,description,items,lines,parties,active_posting_id,created_at FROM opening_documents WHERE company_id=(SELECT id FROM company)")
        .fetch_optional(&mut **tx).await?;
    let has_business = business_exists(tx).await?;
    let nonzero:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE quantity<>0 OR inventory_value<>0 OR average_cost<>0)")
        .fetch_one(&mut **tx).await?;
    let posted = row
        .as_ref()
        .is_some_and(|r| r.get::<String, _>("status") == "posted");
    let blocked_reason = if has_business {
        Some(
            "Opening balances require a company without existing business journals or inventory movements",
        )
    } else if posted {
        Some("Opening balances are already posted")
    } else if nonzero {
        Some("Opening balances require zero inventory before initialization")
    } else {
        None
    };
    let mut can_reverse = false;
    let document = if let Some(row) = row {
        let id: Uuid = row.get("id");
        let active: Option<Uuid> = row.get("active_posting_id");
        if let Some(active) = active {
            can_reverse = reversible(tx, active).await?;
        }
        let history:Value=sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(jsonb_build_object(
            'id',p.id,'journal_id',p.journal_id,'posted_at',p.posted_at,'reversed_at',r.created_at,'reason',r.reason,
            'description',p.description,'items',p.items,'lines',p.lines,'parties',p.parties,'inventory_total',p.inventory_total::text,
            'total_debit',p.total_debit::text,'total_credit',p.total_credit::text) ORDER BY p.posted_at,p.document_version),'[]'::jsonb)
            FROM opening_postings p LEFT JOIN posting_reversals r ON r.source_type='opening' AND r.source_id=p.id WHERE p.document_id=$1"#)
            .bind(id).fetch_one(&mut **tx).await?;
        let items: Vec<Item> =
            serde_json::from_value(row.get("items")).map_err(|e| AppError::internal(e.into()))?;
        let lines: Vec<Line> =
            serde_json::from_value(row.get("lines")).map_err(|e| AppError::internal(e.into()))?;
        let inventory = items
            .iter()
            .try_fold(Decimal::ZERO, |sum, i| number(&i.value).map(|v| sum + v))?;
        let debit = lines
            .iter()
            .try_fold(Decimal::ZERO, |sum, l| number(&l.debit).map(|v| sum + v))?;
        let credit = lines
            .iter()
            .try_fold(Decimal::ZERO, |sum, l| number(&l.credit).map(|v| sum + v))?;
        let posted_at = active.and_then(|a| {
            history
                .as_array()
                .and_then(|h| h.iter().find(|p| p["id"] == a.to_string()))
                .map(|p| p["posted_at"].clone())
        });
        json!({"id":id,"status":row.get::<String,_>("status"),"version":row.get::<i32,_>("version"),
            "description":row.get::<String,_>("description"),"created_at":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
            "posted_at":posted_at,"items":items,"lines":lines,"parties":row.get::<Value,_>("parties"),"inventory_total":inventory.to_string(),
            "total_debit":debit.to_string(),"total_credit":credit.to_string(),"balanced":debit>Decimal::ZERO && debit==credit,"history":history})
    } else {
        Value::Null
    };
    Ok(
        json!({"eligible":blocked_reason.is_none(),"blocked_reason":blocked_reason,"can_reverse":can_reverse,"document":document}),
    )
}

async fn read(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let result = envelope(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn save(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<DraftInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let description = crate::supplier_bills::clean_text(&body.description, 1, 500, false)?;
    let mut tx = state.db.begin().await?;
    lock_accounting(&mut tx).await?;
    let row = lock_document(&mut tx).await?;
    if let Some(row) = &row {
        require_version(row, body.version)?;
        if row.get::<String, _>("status") != "draft" {
            return Err(AppError::conflict(
                "Reverse opening balances before editing",
            ));
        }
    } else if body.version != 0 {
        return Err(AppError::conflict(
            "Opening balances changed; reload before continuing",
        ));
    }
    clean_draft(&mut tx).await?;
    let draft = validate(&mut tx, &body.items, &body.lines, &body.parties).await?;
    let items = serde_json::to_value(&draft.items).map_err(|e| AppError::internal(e.into()))?;
    let lines = serde_json::to_value(&draft.lines).map_err(|e| AppError::internal(e.into()))?;
    let parties = json!(draft.parties);
    let id = if let Some(row) = row {
        let id: Uuid = row.get("id");
        sqlx::query("UPDATE opening_documents SET description=$2,items=$3,lines=$4,parties=$5,version=version+1 WHERE id=$1")
            .bind(id).bind(description).bind(items).bind(lines).bind(parties).execute(&mut *tx).await?;
        id
    } else {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO opening_documents(id,company_id,version,description,items,lines,parties) VALUES($1,(SELECT id FROM company),1,$2,$3,$4,$5)")
            .bind(id).bind(description).bind(items).bind(lines).bind(parties).execute(&mut *tx).await?;
        id
    };
    audit(&mut tx,&user,"opening.draft_saved","opening_document",id,json!({"item_count":body.items.len(),"line_count":body.lines.len(),"party_count":body.parties.len(),"version":body.version+1})).await?;
    let result = envelope(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn post_opening(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PostInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("opening.post", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row = lock_document(&mut tx)
        .await?
        .ok_or_else(|| AppError::conflict("Save an opening draft before posting"))?;
    require_version(&row, body.version)?;
    if row.get::<String, _>("status") != "draft" {
        return Err(AppError::conflict("Opening balances are already posted"));
    }
    clean_draft(&mut tx).await?;
    let (items, lines, parties) = draft_inputs(&row)?;
    let draft = validate(&mut tx, &items, &lines, &parties).await?;
    if draft.debit <= Decimal::ZERO || draft.debit != draft.credit || draft.lines.len() < 2 {
        return Err(AppError::bad_request(
            "Opening balances require at least two balanced lines and a positive total",
        ));
    }
    let id: Uuid = row.get("id");
    let posting_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let description: String = row.get("description");
    let items = serde_json::to_value(&draft.items).map_err(|e| AppError::internal(e.into()))?;
    let lines = serde_json::to_value(&draft.lines).map_err(|e| AppError::internal(e.into()))?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'opening',$3,$4)")
        .bind(journal_id).bind(&description).bind(posting_id).bind(user.id).execute(&mut *tx).await?;
    for line in &draft.lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(journal_id).bind(&line.account_code).bind(number(&line.debit)?).bind(number(&line.credit)?).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO opening_postings(id,document_id,document_version,journal_id,description,items,lines,parties,inventory_total,total_debit,total_credit,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(posting_id).bind(id).bind(body.version).bind(journal_id).bind(&description).bind(&items).bind(&lines).bind(json!(draft.parties)).bind(draft.inventory).bind(draft.debit).bind(draft.credit).bind(user.id).execute(&mut *tx).await?;
    for party in &draft.parties {
        let item_id = Uuid::new_v4();
        let amount = number(&party.amount)?;
        sqlx::query("INSERT INTO opening_party_items(id,posting_id,kind,party_id,reference,document_date,due_date,amount,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(item_id).bind(posting_id).bind(&party.kind).bind(party.party_id).bind(&party.reference).bind(opening_date(&party.document_date)?).bind(party.due_date.as_deref().map(opening_date).transpose()?).bind(amount).bind(user.id).execute(&mut *tx).await?;
        if matches!(party.kind.as_str(), "customer_credit" | "supplier_credit") {
            crate::settlements::create_source_fund(
                &mut tx,
                &user,
                crate::settlements::SourceFund {
                    side: if party.kind == "customer_credit" {
                        "customer"
                    } else {
                        "supplier"
                    },
                    party_id: party.party_id,
                    kind: "opening_credit",
                    amount,
                    source_type: "opening_item",
                    source_id: item_id,
                    journal_id,
                    reference: Some(&party.reference),
                },
            )
            .await?;
        }
    }
    for item in &draft.items {
        crate::inventory_extensions::set_context(
            &mut tx,
            item.stock_context.clone().unwrap_or_default(),
        )
        .await?;
        let quantity = number(&item.quantity)?;
        let value = number(&item.value)?;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(item.product_id)
        .bind(quantity)
        .bind(value)
        .bind(domain::average(value, quantity))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,opening_posting_id) VALUES($1,$2,'opening',$3,$4,$5,$6,$7,$8)")
            .bind(Uuid::new_v4()).bind(item.product_id).bind(quantity).bind(number(&item.unit_cost)?).bind(value).bind(&description).bind(user.id).bind(posting_id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE opening_documents SET status='posted',active_posting_id=$2,items=$3,lines=$4,parties=$5,version=version+1 WHERE id=$1")
        .bind(id).bind(posting_id).bind(items).bind(lines).bind(json!(draft.parties)).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "opening.posted",
        "opening_document",
        id,
        json!({"posting_id":posting_id,"journal_id":journal_id,"total":draft.debit.to_string()}),
    )
    .await?;
    let result = envelope(&mut tx).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse_opening(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = crate::supplier_bills::clean_text(&body.reason, 3, 1000, false)?;
    let request = payload("opening.reverse", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row = lock_document(&mut tx)
        .await?
        .ok_or_else(|| AppError::conflict("Opening balances are not posted"))?;
    require_version(&row, body.version)?;
    let posting_id: Uuid = row
        .get::<Option<Uuid>, _>("active_posting_id")
        .ok_or_else(|| AppError::conflict("Opening balances are not posted"))?;
    // Lock all current product rows in the same sorted order as ordinary stock
    // operations, before verifying exact inventory and applying the reversal.
    sqlx::query("SELECT id FROM products ORDER BY id FOR UPDATE")
        .fetch_all(&mut *tx)
        .await?;
    if !reversible(&mut tx, posting_id).await? {
        return Err(AppError::conflict(
            "Opening balances cannot be reversed after other business postings or inventory changes",
        ));
    }
    let posting = sqlx::query("SELECT journal_id,description FROM opening_postings WHERE id=$1")
        .bind(posting_id)
        .fetch_one(&mut *tx)
        .await?;
    let credits:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM opening_party_items WHERE posting_id=$1 AND kind IN ('customer_credit','supplier_credit')").bind(posting_id).fetch_all(&mut *tx).await?;
    for credit in credits {
        crate::settlements::reverse_source_fund(&mut tx, &user, "opening_item", credit, &reason)
            .await?;
    }
    let reversal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'opening',$2,$3,$4)")
        .bind(reversal_id).bind(posting_id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    let journal_id = Uuid::new_v4();
    let description = format!("Reverse opening balances: {reason}");
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal_id).bind(&description).bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$1,account_code,credit,debit FROM journal_lines WHERE journal_id=$2")
        .bind(journal_id).bind(posting.get::<Uuid,_>("journal_id")).execute(&mut *tx).await?;
    let movements=sqlx::query("SELECT id,product_id,quantity,unit_cost,value FROM inventory_movements WHERE opening_posting_id=$1 AND kind='opening' ORDER BY product_id")
        .bind(posting_id).fetch_all(&mut *tx).await?;
    for movement in movements {
        let product_id: Uuid = movement.get("product_id");
        sqlx::query("UPDATE products SET quantity=0,inventory_value=0,average_cost=0 WHERE id=$1")
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,reverses_id,opening_posting_id) VALUES($1,$2,'opening_reversal',$3,$4,$5,$6,$7,$8,$9)")
            .bind(Uuid::new_v4()).bind(product_id).bind(-movement.get::<Decimal,_>("quantity")).bind(movement.get::<Decimal,_>("unit_cost")).bind(movement.get::<Decimal,_>("value")).bind(&reason).bind(user.id).bind(movement.get::<Uuid,_>("id")).bind(posting_id).execute(&mut *tx).await?;
    }
    let id: Uuid = row.get("id");
    sqlx::query("UPDATE opening_documents SET status='draft',active_posting_id=NULL,version=version+1 WHERE id=$1")
        .bind(id).execute(&mut *tx).await?;
    audit(&mut tx,&user,"opening.reversed","opening_document",id,json!({"posting_id":posting_id,"reversal_id":reversal_id,"journal_id":journal_id,"reason":reason})).await?;
    let result = envelope(&mut tx).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
