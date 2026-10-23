use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
    supplier_bills::{clean_text, journal_line},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/landed-costs", get(list).post(create))
        .route("/landed-costs/options", get(options))
        .route("/landed-costs/preview", post(preview))
        .route("/landed-costs/{id}", get(detail))
        .route("/landed-costs/{id}/reverse", post(reverse))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    source_line_id: Uuid,
    amount: String,
    allocation_method: String,
    receipt_line_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_preview: Option<String>,
    idempotency_key: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}
struct Allocation {
    receipt_line_id: Uuid,
    receipt_id: Uuid,
    receipt_number: String,
    product_id: Uuid,
    product_sku: String,
    product_name: String,
    unit: String,
    quantity: Decimal,
    receipt_value: Decimal,
    weight: Decimal,
    remaining_ratio: Decimal,
    allocated: Decimal,
    inventory_amount: Decimal,
    expense_amount: Decimal,
    before_quantity: Decimal,
    before_value: Decimal,
    before_average: Decimal,
    after_value: Decimal,
    after_average: Decimal,
    receipt_movement_seq: i64,
    cutoff_movement_seq: i64,
    cutoff_location_seq: i64,
}
impl Allocation {
    fn document(&self) -> Value {
        json!({"receipt_line_id":self.receipt_line_id,"receipt_id":self.receipt_id,"receipt_number":self.receipt_number,
            "product_id":self.product_id,"product_sku":self.product_sku,"product_name":self.product_name,"unit":self.unit,
            "quantity":self.quantity.to_string(),"receipt_value":self.receipt_value.to_string(),"weight":self.weight.to_string(),
            "remaining_ratio":self.remaining_ratio.to_string(),"allocated":self.allocated.to_string(),
            "inventory_amount":self.inventory_amount.to_string(),"expense_amount":self.expense_amount.to_string(),
            "before_quantity":self.before_quantity.to_string(),"before_value":self.before_value.to_string(),
            "before_average":self.before_average.to_string(),"after_value":self.after_value.to_string(),"after_average":self.after_average.to_string(),
            "receipt_movement_seq":self.receipt_movement_seq,"cutoff_movement_seq":self.cutoff_movement_seq,"cutoff_location_seq":self.cutoff_location_seq})
    }
}
struct Prepared {
    source: Value,
    source_journal_id: Uuid,
    source_type: String,
    source_id: Uuid,
    source_number: String,
    source_account: String,
    amount: Decimal,
    inventory_amount: Decimal,
    expense_amount: Decimal,
    allocations: Vec<Allocation>,
    reference: Option<String>,
    reason: String,
}
impl Prepared {
    fn document(&self, method: &str) -> Value {
        json!({"source":self.source,"amount":self.amount.to_string(),"inventory_amount":self.inventory_amount.to_string(),
            "expense_amount":self.expense_amount.to_string(),"allocation_method":method,
            "reference":self.reference,"reason":self.reason,
            "items":self.allocations.iter().map(Allocation::document).collect::<Vec<_>>(),
            "warnings":["Biaya yang sudah dibukukan dipindahkan dari akun sumber; kas dan utang tidak berubah.",
            "Bagian yang sudah keluar dibebankan ke HPP; bagian yang masih ada menambah nilai stok berdasarkan penyusutan kuantitas rata-rata bergerak.",
            "Pembalikan membutuhkan riwayat stok dan lokasi yang belum berubah sejak alokasi; pembalikan mutasi lama terkait diblokir selama alokasi aktif.",
            "Retur pelanggan atas pengiriman sebelum alokasi dan retur tagihan sumber biaya diblokir selama alokasi terkait masih aktif."]})
    }
    fn preview_token(&self, method: &str) -> String {
        hex::encode(Sha256::digest(self.document(method).to_string().as_bytes()))
    }
}

fn source_document(row: &sqlx::postgres::PgRow) -> Value {
    let blocked: Option<String> = row.get("blocked_reason");
    json!({"line_id":row.get::<Uuid,_>("line_id"),"journal_id":row.get::<Uuid,_>("journal_id"),
        "source_type":row.get::<String,_>("source_type"),"source_id":row.get::<Uuid,_>("source_id"),
        "number":row.get::<String,_>("number"),"account_code":row.get::<String,_>("account_code"),
        "account_name":row.get::<String,_>("account_name"),"system_role":row.get::<String,_>("system_role"),
        "amount":row.get::<Decimal,_>("amount").to_string(),"available":row.get::<Decimal,_>("available").to_string(),
        "eligible":blocked.is_none(),"blocked_reason":blocked})
}
async fn options(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let sources = sqlx::query(
        "SELECT * FROM landed_cost_source_options ORDER BY created_at DESC,line_id LIMIT 500",
    )
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(source_document)
    .collect::<Vec<_>>();
    let targets = sqlx::query("SELECT l.id receipt_line_id,l.receipt_id,r.number receipt_number,l.product_id,l.product_sku,l.product_name,l.unit,l.quantity,l.value,landed_cost_target_blocked_reason(l.id) blocked_reason FROM purchase_receipt_lines l JOIN purchase_receipts r ON r.id=l.receipt_id WHERE l.product_kind='stock' ORDER BY r.created_at DESC,r.id,l.position LIMIT 1000")
        .fetch_all(&mut *tx).await?.iter().map(|r| {
            let blocked:Option<String>=r.get("blocked_reason");
            json!({"receipt_line_id":r.get::<Uuid,_>("receipt_line_id"),"receipt_id":r.get::<Uuid,_>("receipt_id"),"receipt_number":r.get::<String,_>("receipt_number"),
                "product_id":r.get::<Uuid,_>("product_id"),"product_sku":r.get::<String,_>("product_sku"),"product_name":r.get::<String,_>("product_name"),
                "unit":r.get::<String,_>("unit"),"quantity":r.get::<Decimal,_>("quantity").to_string(),"receipt_value":r.get::<Decimal,_>("value").to_string(),
                "eligible":blocked.is_none(),"blocked_reason":blocked})
        }).collect::<Vec<_>>();
    tx.commit().await?;
    Ok(Json(json!({"sources":sources,"targets":targets})))
}

/// The retained fraction of a late additional cost in a moving-average pool.
/// Receipts dilute the pool; shipments remove the proportional cost share.
/// Histories with other disposal/restoration semantics are rejected upstream.
fn retained_fraction(
    initial: Decimal,
    movements: &[(String, Decimal)],
) -> AppResult<(Decimal, Decimal)> {
    let mut pool = initial;
    let mut fraction = Decimal::ONE;
    for (kind, quantity) in movements {
        if *quantity < Decimal::ZERO {
            if kind != "shipment" || pool <= Decimal::ZERO || -*quantity > pool {
                return Err(AppError::conflict(
                    "Riwayat pengeluaran stok tidak cocok dengan saldo penerimaan.",
                ));
            }
            let after = pool + quantity;
            fraction = if after == Decimal::ZERO {
                Decimal::ZERO
            } else {
                fraction * (after / pool)
            };
        } else if !matches!(kind.as_str(), "receipt" | "purchase_receipt")
            || *quantity == Decimal::ZERO
        {
            return Err(AppError::conflict(
                "Retur, penyesuaian, dan pembalikan stok tidak mendukung alokasi historis.",
            ));
        }
        pool += quantity;
    }
    Ok((fraction, pool))
}

async fn prepare(
    tx: &mut Transaction<'_, Postgres>,
    body: &Input,
    lock: bool,
) -> AppResult<Prepared> {
    let currency: String = sqlx::query_scalar("SELECT currency FROM company")
        .fetch_one(&mut **tx)
        .await?;
    if currency != "IDR" {
        return Err(AppError::bad_request(
            "Landed cost Community hanya mendukung mata uang IDR.",
        ));
    }
    let amount = domain::payment_amount(&body.amount).map_err(AppError::bad_request)?;
    if !matches!(body.allocation_method.as_str(), "value" | "quantity")
        || body.receipt_line_ids.is_empty()
        || body.receipt_line_ids.len() > 100
    {
        return Err(AppError::bad_request(
            "Pilih metode value/quantity dan 1–100 baris barang penerimaan.",
        ));
    }
    let unique: BTreeSet<Uuid> = body.receipt_line_ids.iter().copied().collect();
    if unique.len() != body.receipt_line_ids.len() {
        return Err(AppError::bad_request(
            "Baris penerimaan tidak boleh dipilih dua kali.",
        ));
    }
    let reason = clean_text(&body.reason, 3, 1000, true)?;
    let reference = body
        .reference
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| clean_text(s, 1, 200, false))
        .transpose()?;
    if lock {
        sqlx::query("SELECT id FROM journal_lines WHERE id=$1 FOR UPDATE")
            .bind(body.source_line_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    }
    let source = sqlx::query("SELECT * FROM landed_cost_source_options WHERE line_id=$1")
        .bind(body.source_line_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            AppError::bad_request("Baris biaya sumber tidak memenuhi syarat landed cost.")
        })?;
    if let Some(blocked) = source.get::<Option<String>, _>("blocked_reason") {
        return Err(AppError::conflict(blocked));
    }
    if amount > source.get::<Decimal, _>("available") {
        return Err(AppError::conflict(
            "Nominal melebihi sisa biaya sumber yang belum dialokasikan.",
        ));
    }
    let rows=sqlx::query("SELECT l.*,r.number receipt_number,m.movement_seq FROM purchase_receipt_lines l JOIN purchase_receipts r ON r.id=l.receipt_id LEFT JOIN inventory_movements m ON m.id=l.movement_id WHERE l.id=ANY($1) ORDER BY l.product_id,l.id")
        .bind(body.receipt_line_ids.clone()).fetch_all(&mut **tx).await?;
    if rows.len() != body.receipt_line_ids.len() {
        return Err(AppError::not_found());
    }
    let mut seen = BTreeSet::new();
    let mut allocations = Vec::new();
    for row in rows {
        let product_id: Uuid = row.get("product_id");
        if !seen.insert(product_id) {
            return Err(AppError::bad_request(
                "Pilih satu penerimaan per produk dalam satu dokumen landed cost.",
            ));
        }
        let product = sqlx::query(if lock {
            "SELECT quantity,inventory_value,average_cost FROM products WHERE id=$1 FOR UPDATE"
        } else {
            "SELECT quantity,inventory_value,average_cost FROM products WHERE id=$1"
        })
        .bind(product_id)
        .fetch_one(&mut **tx)
        .await?;
        let line_id: Uuid = row.get("id");
        let blocked: Option<String> =
            sqlx::query_scalar("SELECT landed_cost_target_blocked_reason($1)")
                .bind(line_id)
                .fetch_one(&mut **tx)
                .await?;
        if let Some(reason) = blocked {
            return Err(AppError::conflict(reason));
        }
        let quantity: Decimal = row.get("quantity");
        let receipt_value: Decimal = row.get("value");
        let origin: i64 = row.get("movement_seq");
        let movements=sqlx::query("SELECT kind,quantity,movement_seq FROM inventory_movements WHERE product_id=$1 AND movement_seq>$2 ORDER BY movement_seq")
            .bind(product_id).bind(origin).fetch_all(&mut **tx).await?;
        let sequence = movements
            .last()
            .map(|r| r.get("movement_seq"))
            .unwrap_or(origin);
        let changes = movements
            .iter()
            .map(|m| (m.get::<String, _>("kind"), m.get::<Decimal, _>("quantity")))
            .collect::<Vec<_>>();
        let (ratio, replayed) = retained_fraction(
            row.get::<Decimal, _>("before_quantity") + quantity,
            &changes,
        )?;
        let before_quantity: Decimal = product.get("quantity");
        if replayed != before_quantity {
            return Err(AppError::conflict(
                "Saldo stok berbeda dari riwayat penerimaan; periksa konsistensi database.",
            ));
        }
        let location: i64 = sqlx::query_scalar(
            "SELECT COALESCE(max(event_seq),0) FROM stock_location_events WHERE product_id=$1",
        )
        .bind(product_id)
        .fetch_one(&mut **tx)
        .await?;
        let value: Decimal = product.get("inventory_value");
        allocations.push(Allocation {
            receipt_line_id: line_id,
            receipt_id: row.get("receipt_id"),
            receipt_number: row.get("receipt_number"),
            product_id,
            product_sku: row.get("product_sku"),
            product_name: row.get("product_name"),
            unit: row.get("unit"),
            quantity,
            receipt_value,
            weight: if body.allocation_method == "value" {
                receipt_value
            } else {
                quantity
            },
            remaining_ratio: ratio,
            allocated: Decimal::ZERO,
            inventory_amount: Decimal::ZERO,
            expense_amount: Decimal::ZERO,
            before_quantity,
            before_value: value,
            before_average: product.get("average_cost"),
            after_value: value,
            after_average: product.get("average_cost"),
            receipt_movement_seq: origin,
            cutoff_movement_seq: sequence,
            cutoff_location_seq: location,
        });
    }
    if body.allocation_method == "quantity"
        && allocations
            .iter()
            .map(|a| a.unit.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    {
        return Err(AppError::bad_request(
            "Alokasi menurut kuantitas memerlukan satuan dasar yang sama; gunakan metode nilai untuk satuan berbeda.",
        ));
    }
    let total_weight: Decimal = allocations.iter().map(|a| a.weight).sum();
    let mut cumulative = Decimal::ZERO;
    let mut prior = Decimal::ZERO;
    let count = allocations.len();
    for (index, a) in allocations.iter_mut().enumerate() {
        cumulative += a.weight;
        let through = if index + 1 == count {
            amount
        } else {
            domain::money(amount * (cumulative / total_weight)).min(amount)
        };
        a.allocated = through - prior;
        prior = through;
        a.inventory_amount = domain::money(a.allocated * a.remaining_ratio).min(a.allocated);
        a.expense_amount = a.allocated - a.inventory_amount;
        a.after_value = a.before_value + a.inventory_amount;
        if a.after_value > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request(
                "Nilai persediaan per produk maksimal Rp1 triliun.",
            ));
        }
        a.after_average = if a.inventory_amount > Decimal::ZERO {
            domain::average(a.after_value, a.before_quantity)
        } else {
            a.before_average
        };
        if a.after_average > Decimal::new(999_999_999_999_999_999, 4) {
            return Err(AppError::bad_request(
                "Biaya rata-rata setelah alokasi melebihi kapasitas nilai persediaan; kurangi nominal.",
            ));
        }
    }
    Ok(Prepared {
        source: source_document(&source),
        source_journal_id: source.get("journal_id"),
        source_type: source.get("source_type"),
        source_id: source.get("source_id"),
        source_number: source.get("number"),
        source_account: source.get("account_code"),
        amount,
        inventory_amount: allocations.iter().map(|a| a.inventory_amount).sum(),
        expense_amount: allocations.iter().map(|a| a.expense_amount).sum(),
        allocations,
        reference,
        reason,
    })
}

async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Input>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let prepared = prepare(&mut tx, &body, false).await?;
    tx.commit().await?;
    let mut result = prepared.document(&body.allocation_method);
    result["preview_token"] = json!(prepared.preview_token(&body.allocation_method));
    Ok(Json(result))
}

async fn link_locations(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    product: Uuid,
    after: i64,
    phase: &str,
) -> AppResult<i64> {
    sqlx::query("INSERT INTO landed_cost_location_events(landed_cost_id,event_id,phase) SELECT $1,id,$4 FROM stock_location_events WHERE product_id=$2 AND event_seq>$3")
        .bind(id).bind(product).bind(after).bind(phase).execute(&mut **tx).await?;
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(max(event_seq),0) FROM stock_location_events WHERE product_id=$1",
    )
    .bind(product)
    .fetch_one(&mut **tx)
    .await?)
}
async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Input>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("landed_cost.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let mut p = prepare(&mut tx, &body, true).await?;
    if body.expected_preview.as_deref() != Some(p.preview_token(&body.allocation_method).as_str()) {
        return Err(AppError::conflict(
            "Pratinjau alokasi belum dikonfirmasi atau saldo/biaya berubah. Buat pratinjau ulang sebelum membukukan.",
        ));
    }
    let id = Uuid::new_v4();
    let journal = Uuid::new_v4();
    let sequence: i64 = sqlx::query_scalar("SELECT nextval('landed_cost_number')")
        .fetch_one(&mut *tx)
        .await?;
    let number = format!("LC-{sequence:06}");
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'landed_cost',$3,$4)")
        .bind(journal).bind(format!("Landed cost {number} / {}",p.source_number)).bind(id).bind(user.id).execute(&mut *tx).await?;
    journal_line(&mut tx, journal, "1300", p.inventory_amount).await?;
    journal_line(&mut tx, journal, "5000", p.expense_amount).await?;
    journal_line(&mut tx, journal, &p.source_account, -p.amount).await?;
    sqlx::query("INSERT INTO landed_costs(id,number,source_line_id,source_journal_id,source_type,source_id,source_number,source_account,allocation_method,amount,inventory_amount,expense_amount,reference,reason,journal_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
        .bind(id).bind(number).bind(body.source_line_id).bind(p.source_journal_id).bind(&p.source_type).bind(p.source_id).bind(&p.source_number).bind(&p.source_account)
        .bind(&body.allocation_method).bind(p.amount).bind(p.inventory_amount).bind(p.expense_amount).bind(&p.reference).bind(&p.reason).bind(journal).bind(user.id).execute(&mut *tx).await?;
    for (index, a) in p.allocations.iter_mut().enumerate() {
        if a.inventory_amount > Decimal::ZERO {
            sqlx::query("UPDATE products SET inventory_value=$2,average_cost=$3 WHERE id=$1")
                .bind(a.product_id)
                .bind(a.after_value)
                .bind(a.after_average)
                .execute(&mut *tx)
                .await?;
            sqlx::query("SELECT stock_rebalance($1,NULL,NULL,$2)")
                .bind(a.product_id)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
            a.cutoff_location_seq =
                link_locations(&mut tx, id, a.product_id, a.cutoff_location_seq, "post").await?;
        }
        sqlx::query("INSERT INTO landed_cost_lines(id,landed_cost_id,position,receipt_line_id,product_id,product_sku,product_name,unit,receipt_number,quantity,receipt_value,weight,remaining_ratio,allocated,inventory_amount,expense_amount,before_quantity,before_value,before_average,after_value,after_average,receipt_movement_seq,cutoff_movement_seq,cutoff_location_seq) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)")
            .bind(Uuid::new_v4()).bind(id).bind((index+1)as i32).bind(a.receipt_line_id).bind(a.product_id).bind(&a.product_sku).bind(&a.product_name).bind(&a.unit).bind(&a.receipt_number)
            .bind(a.quantity).bind(a.receipt_value).bind(a.weight).bind(a.remaining_ratio).bind(a.allocated).bind(a.inventory_amount).bind(a.expense_amount).bind(a.before_quantity).bind(a.before_value).bind(a.before_average)
            .bind(a.after_value).bind(a.after_average).bind(a.receipt_movement_seq).bind(a.cutoff_movement_seq).bind(a.cutoff_location_seq).execute(&mut *tx).await?;
    }
    let result = read(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "landed_cost.created",
        "landed_cost",
        id,
        json!({"journal_id":journal,"source_line_id":body.source_line_id,"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

const DOCUMENT: &str = "SELECT jsonb_build_object('id',c.id,'number',c.number,'source_line_id',c.source_line_id,'source_journal_id',c.source_journal_id,'source_type',c.source_type,'source_id',c.source_id,'source_number',c.source_number,'source_account',c.source_account,'source',jsonb_build_object('line_id',c.source_line_id,'journal_id',c.source_journal_id,'source_type',c.source_type,'source_id',c.source_id,'number',c.source_number,'account_code',c.source_account,'account_name',(SELECT name FROM accounts WHERE code=c.source_account)),'allocation_method',c.allocation_method,'amount',c.amount::text,'inventory_amount',c.inventory_amount::text,'expense_amount',c.expense_amount::text,'reference',c.reference,'reason',c.reason,'journal_id',c.journal_id,'created_at',c.created_at,'created_by',c.created_by,'status',CASE WHEN v.id IS NULL THEN 'posted' ELSE 'reversed' END,'version',CASE WHEN v.id IS NULL THEN 1 ELSE 2 END,'can_reverse',landed_cost_reversal_blocked_reason(c.id) IS NULL,'reversal_blocked_reason',landed_cost_reversal_blocked_reason(c.id),'reversal',CASE WHEN v.id IS NULL THEN NULL ELSE jsonb_build_object('id',v.id,'reason',v.reason,'created_at',v.created_at,'journal_id',j.id) END,'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',l.id,'receipt_line_id',l.receipt_line_id,'receipt_id',r.receipt_id,'receipt_number',l.receipt_number,'product_id',l.product_id,'product_sku',l.product_sku,'product_name',l.product_name,'unit',l.unit,'quantity',l.quantity::text,'receipt_value',l.receipt_value::text,'weight',l.weight::text,'remaining_ratio',l.remaining_ratio::text,'allocated',l.allocated::text,'inventory_amount',l.inventory_amount::text,'expense_amount',l.expense_amount::text,'before_quantity',l.before_quantity::text,'before_value',l.before_value::text,'before_average',l.before_average::text,'after_value',l.after_value::text,'after_average',l.after_average::text,'receipt_movement_seq',l.receipt_movement_seq,'cutoff_movement_seq',l.cutoff_movement_seq,'cutoff_location_seq',l.cutoff_location_seq) ORDER BY l.position) FROM landed_cost_lines l JOIN purchase_receipt_lines r ON r.id=l.receipt_line_id WHERE l.landed_cost_id=c.id),'[]'::jsonb),'location_events',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',e.id,'product_id',e.product_id,'warehouse_id',e.warehouse_id,'lot_id',e.lot_id,'value',e.value::text,'phase',m.phase,'created_at',e.created_at) ORDER BY e.event_seq) FROM landed_cost_location_events m JOIN stock_location_events e ON e.id=m.event_id WHERE m.landed_cost_id=c.id),'[]'::jsonb)) FROM landed_costs c LEFT JOIN posting_reversals v ON v.source_type='landed_cost' AND v.source_id=c.id LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=v.id";
async fn read(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Value> {
    sqlx::query_scalar(&format!("{DOCUMENT} WHERE c.id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)
}
async fn detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let result = read(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn list(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Vec<Value> = sqlx::query_scalar(&format!(
        "{DOCUMENT} ORDER BY c.created_at DESC,c.id DESC LIMIT 200"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({"items":items})))
}
async fn reverse(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let reason = clean_text(&body.reason, 3, 1000, true)?;
    let request = payload("landed_cost.reverse", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let source = sqlx::query("SELECT number,journal_id FROM landed_costs WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    let reversed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='landed_cost' AND source_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    if body.version != 1 || reversed {
        return Err(AppError::conflict(
            "Versi landed cost berubah atau dokumen sudah dibalik; muat ulang.",
        ));
    }
    let lines =
        sqlx::query("SELECT * FROM landed_cost_lines WHERE landed_cost_id=$1 ORDER BY product_id")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    for line in &lines {
        sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
            .bind(line.get::<Uuid, _>("product_id"))
            .fetch_one(&mut *tx)
            .await?;
    }
    let blocked: Option<String> =
        sqlx::query_scalar("SELECT landed_cost_reversal_blocked_reason($1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(blocked) = blocked {
        return Err(AppError::conflict(blocked));
    }
    let reversal = Uuid::new_v4();
    let journal = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'landed_cost',$2,$3,$4)")
        .bind(reversal).bind(id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal).bind(format!("Pembalikan landed cost {}",source.get::<String,_>("number"))).bind(reversal).bind(user.id).execute(&mut *tx).await?;
    let postings = sqlx::query(
        "SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY id",
    )
    .bind(source.get::<Uuid, _>("journal_id"))
    .fetch_all(&mut *tx)
    .await?;
    for line in postings {
        journal_line(
            &mut tx,
            journal,
            &line.get::<String, _>("account_code"),
            line.get::<Decimal, _>("credit") - line.get::<Decimal, _>("debit"),
        )
        .await?;
    }
    for line in lines {
        if line.get::<Decimal, _>("inventory_amount") == Decimal::ZERO {
            continue;
        }
        let product: Uuid = line.get("product_id");
        let before: i64 = sqlx::query_scalar(
            "SELECT COALESCE(max(event_seq),0) FROM stock_location_events WHERE product_id=$1",
        )
        .bind(product)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("UPDATE products SET inventory_value=$2,average_cost=$3 WHERE id=$1")
            .bind(product)
            .bind(line.get::<Decimal, _>("before_value"))
            .bind(line.get::<Decimal, _>("before_average"))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT stock_rebalance($1,NULL,NULL,$2)")
            .bind(product)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        link_locations(&mut tx, id, product, before, "reverse").await?;
    }
    let result = read(&mut tx, id).await?;
    audit(
        &mut tx,
        &user,
        "landed_cost.reversed",
        "landed_cost",
        id,
        json!({"reason":reason,"journal_id":journal,"after":result}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    #[test]
    fn depletion_replenishment_and_final_drain_preserve_fraction() {
        let changes = vec![
            ("shipment".into(), d("-4")),
            ("purchase_receipt".into(), d("6")),
            ("shipment".into(), d("-3")),
        ];
        let (fraction, qty) = retained_fraction(d("10"), &changes).unwrap();
        assert_eq!(fraction, d("0.45"));
        assert_eq!(qty, d("9"));
        let (fraction, qty) = retained_fraction(
            d("3"),
            &[("shipment".into(), d("-3")), ("receipt".into(), d("5"))],
        )
        .unwrap();
        assert_eq!(fraction, Decimal::ZERO);
        assert_eq!(qty, d("5"));
    }
    #[test]
    fn depletion_rejects_ambiguous_history_and_overdraw() {
        for changes in [
            vec![("shipment".into(), d("-4"))],
            vec![("supplier_return".into(), d("-1"))],
            vec![("shipment_reversal".into(), d("1"))],
        ] {
            assert!(retained_fraction(d("3"), &changes).is_err());
        }
    }
}
