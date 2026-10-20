use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    domain,
    error::{AppError, AppResult},
};

const MAX_BYTES: usize = 256 * 1024;
const MAX_ROWS: usize = 1000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/imports", get(history))
        .route("/imports/{kind}/template", get(template))
        // JSON escaping can expand an otherwise valid 256 KiB CSV sixfold.
        // Only this endpoint gets a larger envelope; raw CSV remains bounded.
        .route(
            "/imports/{kind}/preview",
            post(preview).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/imports/{id}", get(detail))
        .route("/imports/{id}/commit", post(commit))
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Customers,
    Suppliers,
    Products,
}

impl Kind {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "customers" => Ok(Self::Customers),
            "suppliers" => Ok(Self::Suppliers),
            "products" => Ok(Self::Products),
            _ => Err(AppError::bad_request(
                "Jenis impor harus customers, suppliers, atau products.",
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Customers => "customers",
            Self::Suppliers => "suppliers",
            Self::Products => "products",
        }
    }

    fn authorize(self, user: &AuthUser) -> AppResult<()> {
        user.require(&[match self {
            Self::Customers => "sales",
            Self::Suppliers | Self::Products => "warehouse",
        }])
    }

    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Customers => &["name", "email", "phone", "address"],
            Self::Suppliers => &["code", "name", "email", "phone", "address"],
            Self::Products => &["sku", "name", "kind", "unit", "price"],
        }
    }

    fn required(self) -> &'static [&'static str] {
        match self {
            Self::Customers => &["name"],
            Self::Suppliers => &["code", "name"],
            Self::Products => &["sku", "name", "price"],
        }
    }

    fn identity(self) -> &'static str {
        match self {
            Self::Customers => "name",
            Self::Suppliers => "code",
            Self::Products => "sku",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Issue {
    row: usize,
    column: String,
    message: String,
}

fn issue(row: usize, column: &str, message: impl Into<String>) -> Issue {
    Issue {
        row,
        // PostgreSQL jsonb cannot represent NUL, even in an invalid preview.
        column: column.replace('\0', "\\0"),
        message: message.into(),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlanRow {
    row: usize,
    data: BTreeMap<String, String>,
    errors: Vec<Issue>,
}

#[derive(Debug)]
struct CsvRecord {
    line: usize,
    fields: Vec<String>,
}

// This bounded RFC-4180-style parser accepts either explicit delimiter, including
// escaped quotes and multiline quoted fields. It rejects ambiguous loose quotes.
fn parse_csv(input: &str, delimiter: char) -> Result<Vec<CsvRecord>, Issue> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut chars = input.chars().peekable();
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut line = 1;
    let mut start_line = 1;
    let mut quoted = false;
    let mut after_quote = false;
    let mut started = false;
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => {
                    quoted = false;
                    after_quote = true;
                }
                '\r' | '\n' => {
                    if ch == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    field.push('\n');
                    line += 1;
                }
                _ => field.push(ch),
            }
            continue;
        }
        if ch == delimiter {
            fields.push(std::mem::take(&mut field));
            after_quote = false;
            started = true;
        } else if matches!(ch, '\r' | '\n') {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            fields.push(std::mem::take(&mut field));
            if started || fields.len() > 1 || !fields[0].is_empty() {
                records.push(CsvRecord {
                    line: start_line,
                    fields: std::mem::take(&mut fields),
                });
                if records.len() > MAX_ROWS + 1 {
                    return Err(issue(
                        start_line,
                        "csv",
                        "Maksimal 1.000 baris data per impor.",
                    ));
                }
            } else {
                fields.clear();
            }
            line += 1;
            start_line = line;
            after_quote = false;
            started = false;
        } else if ch == '"' && field.is_empty() && !after_quote {
            quoted = true;
            started = true;
        } else {
            if after_quote || ch == '"' {
                return Err(issue(
                    line,
                    "csv",
                    "Tanda kutip CSV tidak valid; gunakan kutip ganda untuk seluruh isi kolom dan gandakan kutip di dalamnya.",
                ));
            }
            field.push(ch);
            started = true;
        }
    }
    if quoted {
        return Err(issue(
            start_line,
            "csv",
            "Kolom CSV bertanda kutip belum ditutup.",
        ));
    }
    if started || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(CsvRecord {
            line: start_line,
            fields,
        });
    }
    if records.len() > MAX_ROWS + 1 {
        return Err(issue(
            start_line,
            "csv",
            "Maksimal 1.000 baris data per impor.",
        ));
    }
    Ok(records)
}

fn field_text(input: &str, required: bool, max: usize, multiline: bool) -> Result<String, String> {
    let value = input.trim();
    if (required && value.is_empty())
        || value.chars().count() > max
        || value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(format!(
            "Isi {}maksimal {max} karakter dan tanpa karakter kontrol yang tidak didukung.",
            if required { "wajib, " } else { "" }
        ));
    }
    Ok(value.to_owned())
}

fn normalize_row(kind: Kind, row: usize, input: &BTreeMap<String, String>) -> PlanRow {
    let mut data = BTreeMap::new();
    let mut errors = Vec::new();
    for &column in kind.columns() {
        let value = input.get(column).map(String::as_str).unwrap_or("");
        let value = match column {
            "sku" | "code" => value.trim().to_uppercase(),
            "kind" if value.trim().is_empty() => "stock".to_owned(),
            "unit" if value.trim().is_empty() => "pcs".to_owned(),
            _ => value.trim().to_owned(),
        };
        let validation = match column {
            "price" => {
                domain::decimal(&value, 2, "Harga").map(|value| value.normalize().to_string())
            }
            "kind" => crate::business::product_kind(&value)
                .map(str::to_owned)
                .map_err(|e| e.message),
            "sku" | "code" | "phone" => field_text(&value, column != "phone", 64, false),
            "unit" => field_text(&value, true, 32, false),
            "email" => field_text(&value, false, 254, false),
            "address" => field_text(&value, false, 1000, true),
            _ => field_text(&value, true, 200, false),
        };
        match validation {
            Ok(value) => {
                data.insert(column.to_owned(), value);
            }
            Err(message) => {
                data.insert(column.to_owned(), value.replace('\0', "\\0"));
                errors.push(issue(row, column, message));
            }
        }
    }
    PlanRow { row, data, errors }
}

fn make_plan(kind: Kind, csv: &str, delimiter: char) -> (Vec<PlanRow>, Vec<Issue>) {
    let records = match parse_csv(csv, delimiter) {
        Ok(records) => records,
        Err(error) => return (Vec::new(), vec![error]),
    };
    let Some(header) = records.first() else {
        return (
            Vec::new(),
            vec![issue(
                1,
                "csv",
                "File CSV kosong; tambahkan header dan data.",
            )],
        );
    };
    let header_line = header.line;
    let columns: Vec<_> = header
        .fields
        .iter()
        .map(|value| value.trim().to_owned())
        .collect();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for column in &columns {
        if !kind.columns().contains(&column.as_str()) {
            errors.push(issue(
                header_line,
                column,
                "Nama kolom tidak didukung; gunakan header dari template.",
            ));
        } else if !seen.insert(column) {
            errors.push(issue(
                header_line,
                column,
                "Nama kolom muncul lebih dari sekali.",
            ));
        }
    }
    for &required in kind.required() {
        if !columns.iter().any(|column| column == required) {
            errors.push(issue(
                header_line,
                required,
                "Header wajib tidak ditemukan.",
            ));
        }
    }
    if !errors.is_empty() {
        return (Vec::new(), errors);
    }
    let mut rows = Vec::new();
    for record in records.into_iter().skip(1) {
        let input = columns
            .iter()
            .zip(&record.fields)
            .map(|(column, value)| (column.clone(), value.clone()))
            .collect();
        let mut row = normalize_row(kind, record.line, &input);
        if record.fields.len() != columns.len() {
            row.errors.push(issue(
                record.line,
                "csv",
                format!(
                    "Jumlah kolom harus {}, ditemukan {}.",
                    columns.len(),
                    record.fields.len()
                ),
            ));
        }
        rows.push(row);
    }
    if rows.is_empty() {
        errors.push(issue(
            header_line,
            "csv",
            "CSV harus berisi paling sedikit satu baris data.",
        ));
    }
    (rows, errors)
}

// Use the exact same Rust Unicode normalization for existing and incoming values.
// The table lock at commit makes this check cover concurrent manual creates/edits.
fn identity(value: &str) -> String {
    value.trim().to_uppercase()
}

async fn check_duplicates(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    rows: &mut [PlanRow],
) -> AppResult<()> {
    let query = match kind {
        Kind::Customers => "SELECT name FROM customers",
        Kind::Suppliers => "SELECT code FROM suppliers",
        Kind::Products => "SELECT sku FROM products",
    };
    let existing: BTreeSet<String> = sqlx::query_scalar::<_, String>(query)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|v| identity(&v))
        .collect();
    let mut seen = BTreeSet::new();
    for row in rows {
        let column = kind.identity();
        let value = identity(&row.data[column]);
        if value.is_empty() {
            continue;
        }
        if !seen.insert(value.clone()) {
            row.errors.push(issue(
                row.row,
                column,
                "Identitas yang sama sudah ada pada baris sebelumnya di file ini.",
            ));
        }
        if existing.contains(&value) {
            row.errors.push(issue(
                row.row,
                column,
                "Identitas ini sudah ada. Impor hanya menambah data; tidak menimpa data lama.",
            ));
        }
    }
    Ok(())
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFilter {
    delimiter: Option<String>,
}

async fn template(
    user: AuthUser,
    Path(kind): Path<String>,
    Query(filter): Query<TemplateFilter>,
) -> AppResult<(HeaderMap, String)> {
    let kind = Kind::parse(&kind)?;
    kind.authorize(&user)?;
    let csv = match kind {
        Kind::Customers => {
            "name,email,phone,address\r\nPelanggan Contoh,pelanggan@example.test,08123456789,Jakarta\r\n"
        }
        Kind::Suppliers => {
            "code,name,email,phone,address\r\nSUP-CONTOH,Supplier Contoh,supplier@example.test,08123456789,Jakarta\r\n"
        }
        Kind::Products => {
            "sku,name,kind,unit,price\r\nPROD-CONTOH,Produk Contoh,stock,pcs,10000\r\n"
        }
    };
    let csv = match filter.delimiter.as_deref().unwrap_or("comma") {
        "comma" => csv.to_owned(),
        "semicolon" => csv.replace(',', ";"),
        _ => return Err(AppError::bad_request("Pemisah harus comma atau semicolon.")),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"haruflow-{}-template.csv\"",
            kind.name()
        ))
        .map_err(|e| AppError::internal(e.into()))?,
    );
    Ok((headers, format!("\u{feff}{csv}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    filename: String,
    csv: String,
    delimiter: String,
}

async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(kind): Path<String>,
    Json(body): Json<PreviewInput>,
) -> AppResult<Json<Value>> {
    let kind = Kind::parse(&kind)?;
    kind.authorize(&user)?;
    let filename = field_text(&body.filename, true, 120, false).map_err(AppError::bad_request)?;
    if body.csv.len() > MAX_BYTES {
        return Err(AppError::bad_request("Ukuran CSV maksimal 256 KiB."));
    }
    let delimiter = match body.delimiter.as_str() {
        "comma" => ',',
        "semicolon" => ';',
        _ => return Err(AppError::bad_request("Pemisah harus comma atau semicolon.")),
    };
    let (mut rows, mut errors) = make_plan(kind, &body.csv, delimiter);
    let mut tx = state.db.begin().await?;
    check_duplicates(&mut tx, kind, &mut rows).await?;
    for row in &rows {
        errors.extend(row.errors.iter().cloned());
    }
    let valid_count = rows.iter().filter(|row| row.errors.is_empty()).count() as i32;
    let status = if errors.is_empty() {
        "ready"
    } else {
        "invalid"
    };
    let hash = hex::encode(Sha256::digest(
        json!({"kind":kind.name(),"rows":rows,"errors":errors})
            .to_string()
            .as_bytes(),
    ));
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO master_import_batches(id,kind,filename,delimiter,plan_hash,rows,errors,row_count,valid_count,status,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(id).bind(kind.name()).bind(filename).bind(body.delimiter).bind(&hash)
        .bind(json!(rows)).bind(json!(errors)).bind(rows.len() as i32).bind(valid_count).bind(status).bind(user.id).execute(&mut *tx).await?;
    audit(&mut tx,&user,"import.previewed","import",id,json!({"kind":kind.name(),"plan_hash":hash,"row_count":rows.len(),"valid_count":valid_count,"error_count":errors.len()})).await?;
    let result = read_detail(&mut tx, id, &user).await?;
    tx.commit().await?;
    Ok(Json(result))
}

const METADATA: &str = "jsonb_build_object('id',b.id,'kind',b.kind,'filename',b.filename,'delimiter',b.delimiter,'hash',b.plan_hash,'created_at',b.created_at,'status',b.status,'row_count',b.row_count,'valid_count',b.valid_count,'created_count',b.created_count,'committed_at',b.committed_at,'creator',jsonb_build_object('id',u.id,'name',u.name,'email',u.email),'can_commit',b.status='ready' AND b.created_by=$1)";

async fn read_detail(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    user: &AuthUser,
) -> AppResult<Value> {
    let sql = format!(
        "SELECT {METADATA} || jsonb_build_object('rows',b.rows,'errors',b.errors) FROM master_import_batches b JOIN users u ON u.id=b.created_by WHERE b.id=$2"
    );
    let mut result: Value = sqlx::query_scalar(&sql)
        .bind(user.id)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let kind = Kind::parse(result["kind"].as_str().unwrap())?;
    kind.authorize(user)?;
    result["can_commit"] =
        json!(result["can_commit"].as_bool().unwrap() && kind.authorize(user).is_ok());
    Ok(result)
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryFilter {
    kind: Option<String>,
}

async fn history(
    State(state): State<AppState>,
    user: AuthUser,
    Query(filter): Query<HistoryFilter>,
) -> AppResult<Json<Value>> {
    let kinds = if let Some(kind) = filter.kind {
        let kind = Kind::parse(&kind)?;
        kind.authorize(&user)?;
        vec![kind.name().to_owned()]
    } else {
        [Kind::Customers, Kind::Suppliers, Kind::Products]
            .into_iter()
            .filter(|kind| kind.authorize(&user).is_ok())
            .map(|kind| kind.name().to_owned())
            .collect::<Vec<_>>()
    };
    if kinds.is_empty() {
        return Err(AppError::forbidden());
    }
    let sql = format!(
        "SELECT COALESCE(jsonb_agg(x.item ORDER BY x.created_at DESC,x.id DESC),'[]'::jsonb) FROM (SELECT b.created_at,b.id,{METADATA} item FROM master_import_batches b JOIN users u ON u.id=b.created_by WHERE b.kind=ANY($2) ORDER BY b.created_at DESC,b.id DESC LIMIT 50) x"
    );
    let items: Value = sqlx::query_scalar(&sql)
        .bind(user.id)
        .bind(kinds)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({"items":items})))
}

async fn detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let result = read_detail(&mut tx, id, &user).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommitInput {
    idempotency_key: Uuid,
}

async fn insert_master(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    row: &PlanRow,
) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    let data = &row.data;
    let optional = |column: &str| data.get(column).filter(|v| !v.is_empty()).cloned();
    match kind {
        Kind::Customers => {
            sqlx::query(
                "INSERT INTO customers(id,name,email,phone,address) VALUES($1,$2,$3,$4,$5)",
            )
            .bind(id)
            .bind(&data["name"])
            .bind(optional("email"))
            .bind(optional("phone"))
            .bind(optional("address"))
            .execute(&mut **tx)
            .await?;
        }
        Kind::Suppliers => {
            sqlx::query(
                "INSERT INTO suppliers(id,code,name,email,phone,address) VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(id)
            .bind(&data["code"])
            .bind(&data["name"])
            .bind(optional("email"))
            .bind(optional("phone"))
            .bind(optional("address"))
            .execute(&mut **tx)
            .await?;
        }
        Kind::Products => {
            let price =
                domain::decimal(&data["price"], 2, "Harga").map_err(AppError::bad_request)?;
            sqlx::query(
                "INSERT INTO products(id,sku,name,kind,unit,price) VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(id)
            .bind(&data["sku"])
            .bind(&data["name"])
            .bind(&data["kind"])
            .bind(&data["unit"])
            .bind(price)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(id)
}

async fn commit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CommitInput>,
) -> AppResult<Json<Value>> {
    let request = payload("import.commit", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    // Check ownership and role before serving a cached response as well.
    let batch =
        sqlx::query("SELECT kind,created_by,status,rows FROM master_import_batches WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    let kind = Kind::parse(batch.get::<&str, _>("kind"))?;
    kind.authorize(&user)?;
    if batch.get::<Uuid, _>("created_by") != user.id {
        return Err(AppError::forbidden());
    }
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let status: String =
        sqlx::query_scalar("SELECT status FROM master_import_batches WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if status != "ready" {
        return Err(AppError::conflict(
            "Preview tidak siap atau sudah pernah disimpan; buat preview baru untuk memperbaiki data.",
        ));
    }
    let saved_rows: Vec<PlanRow> =
        serde_json::from_value(batch.get("rows")).map_err(|e| AppError::internal(e.into()))?;
    let mut rows: Vec<_> = saved_rows
        .iter()
        .map(|row| normalize_row(kind, row.row, &row.data))
        .collect();
    let lock = match kind {
        Kind::Customers => "LOCK TABLE customers IN SHARE ROW EXCLUSIVE MODE",
        Kind::Suppliers => "LOCK TABLE suppliers IN SHARE ROW EXCLUSIVE MODE",
        Kind::Products => "LOCK TABLE products IN SHARE ROW EXCLUSIVE MODE",
    };
    sqlx::query(lock).execute(&mut *tx).await?;
    check_duplicates(&mut tx, kind, &mut rows).await?;
    if let Some(error) = rows.iter().flat_map(|row| &row.errors).next() {
        return Err(AppError::conflict(format!(
            "Baris {}, kolom {}: {} Buat preview baru.",
            error.row, error.column, error.message
        )));
    }
    for row in &rows {
        let master_id = insert_master(&mut tx, kind, row).await?;
        let (action, resource) = match kind {
            Kind::Customers => ("customer.created", "customer"),
            Kind::Suppliers => ("supplier.created", "supplier"),
            Kind::Products => ("product.created", "product"),
        };
        audit(
            &mut tx,
            &user,
            action,
            resource,
            master_id,
            json!({"import_id":id,"source_row":row.row,"identity":row.data[kind.identity()]}),
        )
        .await?;
    }
    sqlx::query("UPDATE master_import_batches SET status='committed',committed_at=now(),created_count=row_count,commit_key=$2 WHERE id=$1")
        .bind(id).bind(body.idempotency_key).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "import.committed",
        "import",
        id,
        json!({"kind":kind.name(),"created_count":rows.len()}),
    )
    .await?;
    let result = read_detail(&mut tx, id, &user).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_supports_bom_quotes_delimiters_and_physical_lines() {
        let csv = "\u{feff}name;email;phone;address\r\n\"Toko \"\"Maju\"\"; Jaya\";;;\"Jalan A\r\nJakarta\"\r\nBerikut;;;Bandung\r\n";
        let (rows, errors) = make_plan(Kind::Customers, csv, ';');
        assert!(errors.is_empty());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].data["name"], "Toko \"Maju\"; Jaya");
        assert_eq!(rows[0].data["address"], "Jalan A\nJakarta");
        assert_eq!(rows[0].row, 2);
        assert_eq!(rows[1].row, 4);
    }

    #[test]
    fn csv_parser_rejects_ambiguous_quotes_headers_and_row_limit() {
        for csv in ["name\n\"unterminated", "name\na\"b", "name\n\"a\"x"] {
            assert!(!make_plan(Kind::Customers, csv, ',').1.is_empty());
        }
        for csv in [
            "name,name\na,b",
            "email\na",
            "name,quantity\na,1",
            "Name\na",
            "name\n",
        ] {
            assert!(!make_plan(Kind::Customers, csv, ',').1.is_empty());
        }
        let excessive = format!("name\n{}", "A\n".repeat(MAX_ROWS + 1));
        assert!(parse_csv(&excessive, ',').is_err());
        let exact = format!("name\n{}", "A\n".repeat(MAX_ROWS));
        assert_eq!(parse_csv(&exact, ',').unwrap().len(), MAX_ROWS + 1);
    }

    #[test]
    fn csv_normalization_preserves_decimal_precision_and_validates_fields() {
        let (rows, errors) = make_plan(
            Kind::Products,
            "sku,name,price,kind,unit\n test , Produk ,1.2300,,\nBAD,Buruk,0,service,pcs\n",
            ',',
        );
        assert!(errors.is_empty());
        assert!(rows[0].errors.is_empty());
        assert_eq!(rows[0].data["sku"], "TEST");
        assert_eq!(rows[0].data["price"], "1.23");
        assert_eq!(rows[0].data["kind"], "stock");
        assert_eq!(rows[0].data["unit"], "pcs");
        assert_eq!(rows[1].errors[0].column, "price");
        assert!(field_text("bad\0value", true, 200, false).is_err());
        assert!(field_text("a\nb", true, 200, false).is_err());
        assert!(field_text("a\nb", false, 1000, true).is_ok());
    }
}
