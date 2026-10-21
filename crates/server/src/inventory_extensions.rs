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
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StockContext {
    pub warehouse_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    #[serde(default)]
    pub allocations: Vec<StockSelection>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StockSelection {
    pub warehouse_id: Uuid,
    pub lot_id: Option<Uuid>,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
}
pub async fn set_context(
    tx: &mut Transaction<'_, Postgres>,
    context: StockContext,
) -> AppResult<()> {
    if context.allocations.len() > 1000 {
        return Err(AppError::bad_request(
            "Maksimal 1.000 alokasi lokasi dalam satu mutasi.",
        ));
    }
    for item in &context.allocations {
        positive(&item.quantity.to_string())?;
    }
    let value = serde_json::to_string(&context).map_err(|e| AppError::internal(e.into()))?;
    sqlx::query("SELECT set_config('haruflow.stock_context',$1,true)")
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
fn positive(value: &str) -> AppResult<Decimal> {
    domain::decimal(value, 3, "Jumlah").map_err(AppError::bad_request)
}
fn note(value: &str) -> AppResult<String> {
    let v = text(value, 1000, "Alasan")?;
    if v.chars().count() < 3 || v.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "Alasan wajib 3–1.000 karakter tanpa karakter kontrol.",
        ));
    }
    Ok(v)
}
fn reference(value: Option<&str>) -> AppResult<Option<String>> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| text(v, 200, "Referensi"))
        .transpose()
}
async fn active_warehouse(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<()> {
    let active: Option<bool> =
        sqlx::query_scalar("SELECT active FROM warehouses WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    if active != Some(true) {
        return Err(AppError::bad_request(
            "Gudang tidak ditemukan atau tidak aktif.",
        ));
    }
    Ok(())
}
async fn lock_product(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM products WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    if row.get::<String, _>("kind") != "stock" {
        return Err(AppError::bad_request(
            "Produk jasa tidak memiliki persediaan.",
        ));
    }
    Ok(row)
}
pub async fn base_quantity(
    tx: &mut Transaction<'_, Postgres>,
    product: Uuid,
    value: &str,
    unit: Option<&str>,
) -> AppResult<Decimal> {
    let amount = positive(value)?;
    let base: String = sqlx::query_scalar("SELECT unit FROM products WHERE id=$1")
        .bind(product)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    if unit.is_none() || unit == Some(base.as_str()) {
        return Ok(amount);
    }
    let conversion = sqlx::query(
        "SELECT numerator,denominator FROM product_units WHERE product_id=$1 AND name=$2",
    )
    .bind(product)
    .bind(unit)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::bad_request("Satuan konversi tidak terdaftar pada produk."))?;
    let numerator: Decimal = conversion.get("numerator");
    let denominator: Decimal = conversion.get("denominator");
    let quantity = amount * numerator / denominator;
    if quantity * denominator != amount * numerator
        || quantity.normalize().scale() > 3
        || quantity > Decimal::from(1_000_000_000u64)
    {
        return Err(AppError::bad_request(
            "Konversi harus menghasilkan jumlah satuan dasar yang tepat, maksimal tiga desimal.",
        ));
    }
    Ok(quantity)
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/inventory/warehouses",
            get(warehouses).post(create_warehouse),
        )
        .route(
            "/inventory/warehouses/{id}",
            axum::routing::patch(update_warehouse),
        )
        .route("/inventory/positions", get(positions))
        .route(
            "/inventory/products/{id}/settings",
            get(settings).put(save_settings),
        )
        .route(
            "/inventory/products/{id}/units",
            get(units).post(create_unit),
        )
        .route("/inventory/lots", get(lots).post(create_lot))
        .route("/inventory/location-receipts", post(receive))
        .route("/inventory/transfers", get(transfers).post(transfer))
        .route("/inventory/assignments", post(assign_lot))
        .route("/inventory/reservations", get(reservations).post(reserve))
        .route("/inventory/reservations/{id}/release", post(release))
        .route("/inventory/location-events", get(location_events))
}

async fn warehouses(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(w) ORDER BY is_default DESC,code),'[]'::jsonb) FROM warehouses w").fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WarehouseInput {
    code: String,
    name: String,
    idempotency_key: Uuid,
}
async fn create_warehouse(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<WarehouseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let code = text(&body.code.trim().to_uppercase(), 32, "Kode gudang")?;
    if !code
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-')
        || !code
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return Err(AppError::bad_request(
            "Kode gudang hanya huruf kapital, angka, garis bawah, dan tanda hubung.",
        ));
    }
    let name = text(&body.name, 120, "Nama gudang")?;
    let request = payload("warehouse.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let id = Uuid::new_v4();
    let result: Value = sqlx::query_scalar(
        "INSERT INTO warehouses(id,code,name) VALUES($1,$2,$3) RETURNING to_jsonb(warehouses)",
    )
    .bind(id)
    .bind(code)
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        &user,
        "warehouse.created",
        "warehouse",
        id,
        json!({}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WarehouseUpdate {
    version: i64,
    name: String,
    active: bool,
    idempotency_key: Uuid,
}
async fn update_warehouse(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WarehouseUpdate>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let name = text(&body.name, 120, "Nama gudang")?;
    let request = payload("warehouse.update", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row = sqlx::query("SELECT * FROM warehouses WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    if row.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict(
            "Gudang berubah; muat ulang sebelum menyimpan.",
        ));
    }
    if !body.active {
        let occupied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM stock_buckets WHERE warehouse_id=$1 AND quantity<>0)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if row.get::<bool, _>("is_default") || occupied {
            return Err(AppError::conflict(
                "Gudang utama atau gudang yang masih menyimpan stok tidak dapat dinonaktifkan.",
            ));
        }
    }
    let result:Value=sqlx::query_scalar("UPDATE warehouses SET name=$2,active=$3,version=version+1 WHERE id=$1 RETURNING to_jsonb(warehouses)").bind(id).bind(name).bind(body.active).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "warehouse.updated",
        "warehouse",
        id,
        json!({"active":body.active}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PositionFilter {
    warehouse_id: Option<Uuid>,
    product_id: Option<Uuid>,
}
async fn positions(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PositionFilter>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',b.id,'product_id',b.product_id,'sku',p.sku,'name',p.name,'unit',p.unit,'warehouse_id',b.warehouse_id,'warehouse_code',w.code,'warehouse_name',w.name,'lot_id',b.lot_id,'lot_code',l.code,'lot_kind',l.kind,'expires_on',l.expires_on,'expired',COALESCE(l.expires_on<(now() AT TIME ZONE c.timezone)::date,false),'quantity',b.quantity::text,'inventory_value',b.inventory_value::text,'reserved',stock_bucket_reserved(b.id)::text,'available',(b.quantity-stock_bucket_reserved(b.id))::text,'version',b.version,'average_cost',p.average_cost::text) ORDER BY w.code,p.sku,l.code NULLS FIRST),'[]'::jsonb)
 FROM stock_buckets b JOIN products p ON p.id=b.product_id JOIN warehouses w ON w.id=b.warehouse_id LEFT JOIN stock_lots l ON l.id=b.lot_id CROSS JOIN company c
 WHERE ($1::uuid IS NULL OR b.warehouse_id=$1) AND ($2::uuid IS NULL OR b.product_id=$2)"#).bind(filter.warehouse_id).bind(filter.product_id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
async fn settings(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('product_id',p.id,'tracking',COALESCE(s.tracking,'none'),'expiry_required',COALESCE(s.expiry_required,false),'version',COALESCE(s.version,1),'unit',p.unit,'kind',p.kind) FROM products p LEFT JOIN product_inventory_settings s ON s.product_id=p.id WHERE p.id=$1").bind(id).fetch_optional(&state.db).await?;
    Ok(Json(result.ok_or_else(AppError::not_found)?))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SettingsInput {
    version: i64,
    tracking: String,
    expiry_required: bool,
    idempotency_key: Uuid,
}
async fn save_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SettingsInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    if !matches!(body.tracking.as_str(), "none" | "lot" | "serial")
        || body.expiry_required && body.tracking == "none"
    {
        return Err(AppError::bad_request(
            "Pelacakan atau aturan kedaluwarsa tidak valid.",
        ));
    }
    let request = payload("inventory.settings", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    lock_product(&mut tx, id).await?;
    sqlx::query(
        "INSERT INTO product_inventory_settings(product_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let row =
        sqlx::query("SELECT * FROM product_inventory_settings WHERE product_id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if row.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict("Pengaturan produk berubah; muat ulang."));
    }
    let incompatible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM stock_buckets b LEFT JOIN stock_lots l ON l.id=b.lot_id WHERE b.product_id=$1 AND b.quantity>0 AND ($2::text<>'none' AND (l.id IS NULL OR l.kind<>$2 OR ($3 AND l.expires_on IS NULL))))").bind(id).bind(&body.tracking).bind(body.expiry_required).fetch_one(&mut *tx).await?;
    if incompatible {
        return Err(AppError::conflict(
            "Stok lama belum memiliki lot/seri yang sesuai. Tetapkan hasil hitung fisik dahulu; sistem tidak menebak lot historis.",
        ));
    }
    if row.get::<String, _>("tracking") != "none"
        && row.get::<String, _>("tracking") != body.tracking
    {
        let occupied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM stock_buckets WHERE product_id=$1 AND quantity>0)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if occupied {
            return Err(AppError::conflict(
                "Habiskan stok sebelum mengganti jenis pelacakan.",
            ));
        }
    }
    let result:Value=sqlx::query_scalar("UPDATE product_inventory_settings SET tracking=$2,expiry_required=$3,version=version+1 WHERE product_id=$1 RETURNING to_jsonb(product_inventory_settings)").bind(id).bind(&body.tracking).bind(body.expiry_required).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "inventory.settings_updated",
        "product",
        id,
        json!({"tracking":body.tracking}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn units(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(u)||jsonb_build_object('numerator',u.numerator::text,'denominator',u.denominator::text) ORDER BY name),'[]'::jsonb) FROM product_units u WHERE product_id=$1").bind(id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnitInput {
    name: String,
    numerator: String,
    denominator: String,
    idempotency_key: Uuid,
}
async fn create_unit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UnitInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let name = text(&body.name, 32, "Satuan")?;
    let numerator =
        domain::decimal(&body.numerator, 0, "Pembilang").map_err(AppError::bad_request)?;
    let denominator =
        domain::decimal(&body.denominator, 0, "Penyebut").map_err(AppError::bad_request)?;
    let request = payload("inventory.unit", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let product = lock_product(&mut tx, id).await?;
    if product.get::<String, _>("unit") == name {
        return Err(AppError::bad_request(
            "Gunakan nama berbeda dari satuan dasar.",
        ));
    }
    let uid = Uuid::new_v4();
    sqlx::query("INSERT INTO product_units(id,product_id,name,numerator,denominator) VALUES($1,$2,$3,$4,$5)").bind(uid).bind(id).bind(&name).bind(numerator).bind(denominator).execute(&mut *tx).await?;
    let result = json!({"id":uid,"product_id":id,"name":name,"numerator":numerator.to_string(),"denominator":denominator.to_string()});
    audit(
        &mut tx,
        &user,
        "inventory.unit_created",
        "product",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn lots(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PositionFilter>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(l) ORDER BY l.expires_on NULLS LAST,l.code),'[]'::jsonb) FROM stock_lots l WHERE ($1::uuid IS NULL OR l.product_id=$1)").bind(filter.product_id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LotInput {
    product_id: Uuid,
    code: String,
    kind: String,
    expires_on: Option<NaiveDate>,
    idempotency_key: Uuid,
}
async fn create_lot(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<LotInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let code = text(&body.code, 100, "Lot/nomor seri")?;
    if !matches!(body.kind.as_str(), "lot" | "serial") || code.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "Jenis atau kode lot/nomor seri tidak valid.",
        ));
    }
    let request = payload("inventory.lot", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    lock_product(&mut tx, body.product_id).await?;
    let id = Uuid::new_v4();
    let result:Value=sqlx::query_scalar("INSERT INTO stock_lots(id,product_id,code,kind,expires_on) VALUES($1,$2,$3,$4,$5) RETURNING to_jsonb(stock_lots)").bind(id).bind(body.product_id).bind(code).bind(&body.kind).bind(body.expires_on).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "inventory.lot_created",
        "stock_lot",
        id,
        json!({"product_id":body.product_id}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

// Posting operations follow below. Every quantity is converted exactly before
// it reaches the global movement and its automatic warehouse/batch trigger.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiptInput {
    product_id: Uuid,
    warehouse_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: String,
    unit: Option<String>,
    unit_cost: String,
    reference: Option<String>,
    #[serde(default)]
    allocations: Vec<StockSelection>,
    idempotency_key: Uuid,
}
async fn receive(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ReceiptInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let entered = positive(&body.quantity)?;
    let cost =
        domain::decimal(&body.unit_cost, 4, "Biaya satuan").map_err(AppError::bad_request)?;
    let value = domain::money(entered * cost);
    if value <= Decimal::ZERO || value > Decimal::from(1_000_000_000_000u64) {
        return Err(AppError::bad_request(
            "Nilai penerimaan harus positif dan maksimal satu triliun.",
        ));
    }
    let reference = reference(body.reference.as_deref())?;
    let request = payload("inventory.location_receipt", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let product = lock_product(&mut tx, body.product_id).await?;
    active_warehouse(&mut tx, body.warehouse_id).await?;
    let quantity = base_quantity(
        &mut tx,
        body.product_id,
        &body.quantity,
        body.unit.as_deref(),
    )
    .await?;
    let before_qty: Decimal = product.get("quantity");
    let before_value: Decimal = product.get("inventory_value");
    let new_qty = before_qty + quantity;
    let new_value = before_value + value;
    if new_qty > Decimal::from(1_000_000_000u64) || new_value > Decimal::from(1_000_000_000_000u64)
    {
        return Err(AppError::bad_request(
            "Penerimaan melampaui batas persediaan produk.",
        ));
    }
    sqlx::query("UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1")
        .bind(body.product_id)
        .bind(new_qty)
        .bind(new_value)
        .bind(domain::average(new_value, new_qty))
        .execute(&mut *tx)
        .await?;
    set_context(
        &mut tx,
        StockContext {
            warehouse_id: Some(body.warehouse_id),
            lot_id: body.lot_id,
            allocations: body.allocations.clone(),
        },
    )
    .await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,before_quantity,before_inventory_value,before_average_cost) VALUES($1,$2,'receipt',$3,$4,$5,$6,$7,$8,$9,$10)")
 .bind(id).bind(body.product_id).bind(quantity).bind(domain::average(value,quantity)).bind(value).bind(reference).bind(user.id).bind(before_qty).bind(before_value).bind(product.get::<Decimal,_>("average_cost")).execute(&mut *tx).await?;
    journal(
        &mut tx,
        &user,
        "receipt",
        id,
        "Penerimaan stok per gudang",
        ("1300", "3000"),
        value,
    )
    .await?;
    let result = json!({"id":id,"product_id":body.product_id,"warehouse_id":body.warehouse_id,"quantity":quantity.to_string(),"entered_quantity":body.quantity,"entered_unit":body.unit.unwrap_or(product.get("unit")),"value":value.to_string()});
    audit(
        &mut tx,
        &user,
        "inventory.location_received",
        "inventory_movement",
        id,
        result.clone(),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn bucket(
    tx: &mut Transaction<'_, Postgres>,
    product: Uuid,
    warehouse: Uuid,
    lot: Option<Uuid>,
) -> AppResult<sqlx::postgres::PgRow> {
    sqlx::query("SELECT b.*,stock_bucket_reserved(b.id) reserved FROM stock_buckets b WHERE product_id=$1 AND warehouse_id=$2 AND lot_id IS NOT DISTINCT FROM $3 FOR UPDATE").bind(product).bind(warehouse).bind(lot).fetch_optional(&mut **tx).await?.ok_or_else(||AppError::bad_request("Posisi stok tidak ditemukan di gudang/lot pilihan."))
}
async fn increment_bucket(
    tx: &mut Transaction<'_, Postgres>,
    product: Uuid,
    warehouse: Uuid,
    lot: Option<Uuid>,
    delta: Decimal,
) -> AppResult<()> {
    sqlx::query("INSERT INTO stock_buckets(id,product_id,warehouse_id,lot_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(Uuid::new_v4()).bind(product).bind(warehouse).bind(lot).execute(&mut **tx).await?;
    sqlx::query("UPDATE stock_buckets SET quantity=quantity+$4,version=version+1 WHERE product_id=$1 AND warehouse_id=$2 AND lot_id IS NOT DISTINCT FROM $3").bind(product).bind(warehouse).bind(lot).bind(delta).execute(&mut **tx).await?;
    Ok(())
}
struct LocationEvent {
    product: Uuid,
    warehouse: Uuid,
    lot: Option<Uuid>,
    quantity: Decimal,
    kind: &'static str,
    transfer: Option<Uuid>,
}
async fn location_event(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    event: LocationEvent,
) -> AppResult<()> {
    sqlx::query("INSERT INTO stock_location_events(id,product_id,warehouse_id,lot_id,kind,quantity,transfer_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::new_v4()).bind(event.product).bind(event.warehouse).bind(event.lot).bind(event.kind).bind(event.quantity).bind(event.transfer).bind(user.id).execute(&mut **tx).await?;
    Ok(())
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TransferItem {
    product_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: String,
    unit: Option<String>,
    version: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TransferInput {
    from_warehouse_id: Uuid,
    to_warehouse_id: Uuid,
    items: Vec<TransferItem>,
    reference: Option<String>,
    reason: String,
    idempotency_key: Uuid,
}
async fn transfer(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<TransferInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let reason = note(&body.reason)?;
    let reference = reference(body.reference.as_deref())?;
    if body.from_warehouse_id == body.to_warehouse_id
        || body.items.is_empty()
        || body.items.len() > 100
    {
        return Err(AppError::bad_request(
            "Pilih dua gudang berbeda dan 1–100 baris transfer.",
        ));
    }
    let request = payload("inventory.transfer", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    active_warehouse(&mut tx, body.from_warehouse_id).await?;
    active_warehouse(&mut tx, body.to_warehouse_id).await?;
    let id = Uuid::new_v4();
    let number: String =
        sqlx::query_scalar("SELECT 'TR-'||nextval('warehouse_transfer_number')::text")
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO warehouse_transfers(id,number,from_warehouse_id,to_warehouse_id,reference,reason,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(&number).bind(body.from_warehouse_id).bind(body.to_warehouse_id).bind(reference).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    let mut items: Vec<_> = body.items.iter().collect();
    items.sort_by_key(|i| (i.product_id, i.lot_id));
    let mut seen = BTreeSet::new();
    for item in items {
        if !seen.insert((item.product_id, item.lot_id)) {
            return Err(AppError::bad_request(
                "Gabungkan produk/lot yang sama dalam satu baris transfer.",
            ));
        }
        lock_product(&mut tx, item.product_id).await?;
        let quantity = base_quantity(
            &mut tx,
            item.product_id,
            &item.quantity,
            item.unit.as_deref(),
        )
        .await?;
        let source = bucket(
            &mut tx,
            item.product_id,
            body.from_warehouse_id,
            item.lot_id,
        )
        .await?;
        if source.get::<i64, _>("version") != item.version {
            return Err(AppError::conflict(
                "Posisi stok berubah; muat ulang sebelum transfer.",
            ));
        }
        if source.get::<Decimal, _>("quantity") - source.get::<Decimal, _>("reserved") < quantity {
            return Err(AppError::conflict(
                "Stok transfer tidak cukup atau telah direservasi.",
            ));
        }
        increment_bucket(
            &mut tx,
            item.product_id,
            body.from_warehouse_id,
            item.lot_id,
            -quantity,
        )
        .await?;
        increment_bucket(
            &mut tx,
            item.product_id,
            body.to_warehouse_id,
            item.lot_id,
            quantity,
        )
        .await?;
        for (warehouse, delta) in [
            (body.from_warehouse_id, -quantity),
            (body.to_warehouse_id, quantity),
        ] {
            location_event(
                &mut tx,
                &user,
                LocationEvent {
                    product: item.product_id,
                    warehouse,
                    lot: item.lot_id,
                    quantity: delta,
                    kind: "transfer",
                    transfer: Some(id),
                },
            )
            .await?;
        }
        sqlx::query("SELECT stock_rebalance($1,NULL,$2,$3)")
            .bind(item.product_id)
            .bind(id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    }
    let result = json!({"id":id,"number":number,"from_warehouse_id":body.from_warehouse_id,"to_warehouse_id":body.to_warehouse_id});
    audit(
        &mut tx,
        &user,
        "inventory.transferred",
        "warehouse_transfer",
        id,
        json!({"items":body.items.len(),"reason":reason}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn transfers(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(t)||jsonb_build_object('from_warehouse_name',a.name,'to_warehouse_name',b.name,'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('product_id',e.product_id,'sku',p.sku,'name',p.name,'lot_id',e.lot_id,'lot_code',l.code,'quantity',(-e.quantity)::text) ORDER BY p.sku,l.code) FROM stock_location_events e JOIN products p ON p.id=e.product_id LEFT JOIN stock_lots l ON l.id=e.lot_id WHERE e.transfer_id=t.id AND e.kind='transfer' AND e.quantity<0),'[]'::jsonb)) ORDER BY t.created_at DESC,t.id DESC),'[]'::jsonb) FROM warehouse_transfers t JOIN warehouses a ON a.id=t.from_warehouse_id JOIN warehouses b ON b.id=t.to_warehouse_id").fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AssignmentInput {
    product_id: Uuid,
    warehouse_id: Uuid,
    from_lot_id: Option<Uuid>,
    to_lot_id: Uuid,
    quantity: String,
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}
async fn assign_lot(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AssignmentInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let quantity = positive(&body.quantity)?;
    let reason = note(&body.reason)?;
    if body.from_lot_id == Some(body.to_lot_id) {
        return Err(AppError::bad_request(
            "Lot tujuan harus berbeda dari lot asal.",
        ));
    }
    let request = payload("inventory.assign_lot", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    lock_product(&mut tx, body.product_id).await?;
    active_warehouse(&mut tx, body.warehouse_id).await?;
    let lot = sqlx::query("SELECT kind FROM stock_lots WHERE id=$1 AND product_id=$2")
        .bind(body.to_lot_id)
        .bind(body.product_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(AppError::not_found)?;
    if lot.get::<String, _>("kind") == "serial" && quantity != Decimal::ONE {
        return Err(AppError::bad_request(
            "Penetapan nomor seri harus tepat satu unit.",
        ));
    }
    let source = bucket(
        &mut tx,
        body.product_id,
        body.warehouse_id,
        body.from_lot_id,
    )
    .await?;
    if source.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict("Posisi stok berubah; muat ulang."));
    }
    if source.get::<Decimal, _>("quantity") - source.get::<Decimal, _>("reserved") < quantity {
        return Err(AppError::conflict(
            "Jumlah penetapan melebihi stok bebas reservasi.",
        ));
    }
    increment_bucket(
        &mut tx,
        body.product_id,
        body.warehouse_id,
        body.from_lot_id,
        -quantity,
    )
    .await?;
    increment_bucket(
        &mut tx,
        body.product_id,
        body.warehouse_id,
        Some(body.to_lot_id),
        quantity,
    )
    .await?;
    for (lot, delta) in [
        (body.from_lot_id, -quantity),
        (Some(body.to_lot_id), quantity),
    ] {
        location_event(
            &mut tx,
            &user,
            LocationEvent {
                product: body.product_id,
                warehouse: body.warehouse_id,
                lot,
                quantity: delta,
                kind: "assignment",
                transfer: None,
            },
        )
        .await?;
    }
    sqlx::query("SELECT stock_rebalance($1,NULL,NULL,$2)")
        .bind(body.product_id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let result = json!({"product_id":body.product_id,"warehouse_id":body.warehouse_id,"to_lot_id":body.to_lot_id,"quantity":quantity.to_string()});
    audit(
        &mut tx,
        &user,
        "inventory.lot_assigned",
        "stock_lot",
        body.to_lot_id,
        json!({"product_id":body.product_id,"quantity":quantity.to_string(),"reason":reason}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReservationInput {
    product_id: Uuid,
    warehouse_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: String,
    unit: Option<String>,
    version: i64,
    order_id: Option<Uuid>,
    expires_at: Option<DateTime<Utc>>,
    reference: Option<String>,
    idempotency_key: Uuid,
}
async fn reserve(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ReservationInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse", "sales"])?;
    if body.expires_at.is_some_and(|v| v <= Utc::now()) {
        return Err(AppError::bad_request(
            "Masa berlaku reservasi harus berada di masa depan.",
        ));
    }
    let reference = reference(body.reference.as_deref())?;
    let request = payload("inventory.reserve", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    lock_product(&mut tx, body.product_id).await?;
    active_warehouse(&mut tx, body.warehouse_id).await?;
    let quantity = base_quantity(
        &mut tx,
        body.product_id,
        &body.quantity,
        body.unit.as_deref(),
    )
    .await?;
    let source = bucket(&mut tx, body.product_id, body.warehouse_id, body.lot_id).await?;
    if source.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict(
            "Posisi stok berubah; muat ulang sebelum reservasi.",
        ));
    }
    if source.get::<Decimal, _>("quantity") - source.get::<Decimal, _>("reserved") < quantity {
        return Err(AppError::conflict(
            "Stok bebas tidak cukup untuk reservasi.",
        ));
    }
    if let Some(lot) = body.lot_id {
        let kind: String = sqlx::query_scalar("SELECT kind FROM stock_lots WHERE id=$1")
            .bind(lot)
            .fetch_one(&mut *tx)
            .await?;
        if kind == "serial" && quantity != Decimal::ONE {
            return Err(AppError::bad_request(
                "Reservasi nomor seri harus tepat satu unit.",
            ));
        }
        let expired:bool=sqlx::query_scalar("SELECT COALESCE(expires_on<(now() AT TIME ZONE (SELECT timezone FROM company))::date,false) FROM stock_lots WHERE id=$1").bind(lot).fetch_one(&mut *tx).await?;
        if expired {
            return Err(AppError::conflict(
                "Lot kedaluwarsa tidak dapat direservasi.",
            ));
        }
    }
    if let Some(order) = body.order_id {
        let remaining:Option<Decimal>=sqlx::query_scalar("SELECT l.quantity-p.shipped FROM sales_order_lines l JOIN sales_orders o ON o.id=l.order_id JOIN sales_line_progress($1)p ON p.line_id=l.id WHERE l.order_id=$1 AND l.product_id=$2 AND o.status NOT IN ('draft','cancelled')").bind(order).bind(body.product_id).fetch_optional(&mut *tx).await?;
        let prior:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(stock_reservation_remaining(r.id)),0) FROM stock_reservations r JOIN stock_buckets b ON b.id=r.bucket_id WHERE r.order_id=$1 AND b.product_id=$2 AND (r.expires_at IS NULL OR r.expires_at>now())").bind(order).bind(body.product_id).fetch_one(&mut *tx).await?;
        if remaining.is_none_or(|left| left - prior < quantity) {
            return Err(AppError::conflict(
                "Reservasi melebihi jumlah pesanan yang belum dikirim.",
            ));
        }
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO stock_reservations(id,bucket_id,quantity,order_id,reference,expires_at,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(source.get::<Uuid,_>("id")).bind(quantity).bind(body.order_id).bind(reference).bind(body.expires_at).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE stock_buckets SET version=version+1 WHERE id=$1")
        .bind(source.get::<Uuid, _>("id"))
        .execute(&mut *tx)
        .await?;
    let result = json!({"id":id,"quantity":quantity.to_string(),"remaining":quantity.to_string(),"version":1});
    audit(&mut tx,&user,"inventory.reserved","stock_reservation",id,json!({"product_id":body.product_id,"quantity":quantity.to_string(),"order_id":body.order_id})).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReleaseInput {
    version: i64,
    quantity: Option<String>,
    reason: String,
    idempotency_key: Uuid,
}
async fn release(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReleaseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse", "sales"])?;
    let reason = note(&body.reason)?;
    let request = payload("inventory.release", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row=sqlx::query("SELECT r.*,stock_reservation_remaining(r.id) remaining FROM stock_reservations r WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(AppError::not_found)?;
    if row.get::<i64, _>("version") != body.version {
        return Err(AppError::conflict("Reservasi berubah; muat ulang."));
    }
    let remaining: Decimal = row.get("remaining");
    let quantity = body
        .quantity
        .as_deref()
        .map(positive)
        .transpose()?
        .unwrap_or(remaining);
    if quantity <= Decimal::ZERO || quantity > remaining {
        return Err(AppError::conflict(
            "Jumlah pelepasan melebihi sisa reservasi.",
        ));
    }
    let serial:bool=sqlx::query_scalar("SELECT COALESCE(l.kind='serial',false) FROM stock_buckets b LEFT JOIN stock_lots l ON l.id=b.lot_id WHERE b.id=$1").bind(row.get::<Uuid,_>("bucket_id")).fetch_one(&mut *tx).await?;
    if serial && quantity != Decimal::ONE {
        return Err(AppError::bad_request(
            "Pelepasan reservasi nomor seri harus satu unit penuh.",
        ));
    }
    sqlx::query("INSERT INTO stock_reservation_events(id,reservation_id,kind,quantity,reason,created_by) VALUES($1,$2,'release',$3,$4,$5)").bind(Uuid::new_v4()).bind(id).bind(quantity).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE stock_reservations SET version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE stock_buckets SET version=version+1 WHERE id=$1")
        .bind(row.get::<Uuid, _>("bucket_id"))
        .execute(&mut *tx)
        .await?;
    let result =
        json!({"id":id,"remaining":(remaining-quantity).to_string(),"version":body.version+1});
    audit(
        &mut tx,
        &user,
        "inventory.reservation_released",
        "stock_reservation",
        id,
        json!({"quantity":quantity.to_string(),"reason":reason}),
    )
    .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn reservations(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PositionFilter>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(r)||jsonb_build_object('quantity',r.quantity::text,'remaining',stock_reservation_remaining(r.id)::text,'expired',r.expires_at IS NOT NULL AND r.expires_at<=now(),'product_id',b.product_id,'warehouse_id',b.warehouse_id,'warehouse_name',w.name,'lot_id',b.lot_id,'lot_code',l.code,'sku',p.sku,'name',p.name,'unit',p.unit,'order_number',o.number) ORDER BY r.created_at DESC,r.id DESC),'[]'::jsonb) FROM stock_reservations r JOIN stock_buckets b ON b.id=r.bucket_id JOIN products p ON p.id=b.product_id JOIN warehouses w ON w.id=b.warehouse_id LEFT JOIN stock_lots l ON l.id=b.lot_id LEFT JOIN sales_orders o ON o.id=r.order_id WHERE ($1::uuid IS NULL OR b.warehouse_id=$1) AND ($2::uuid IS NULL OR b.product_id=$2)").bind(filter.warehouse_id).bind(filter.product_id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
async fn location_events(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<PositionFilter>,
) -> AppResult<Json<Value>> {
    let items:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(e)||jsonb_build_object('quantity',e.quantity::text,'value',e.value::text,'sku',p.sku,'product_name',p.name,'warehouse_name',w.name,'lot_code',l.code) ORDER BY e.event_seq DESC),'[]'::jsonb) FROM stock_location_events e JOIN products p ON p.id=e.product_id JOIN warehouses w ON w.id=e.warehouse_id LEFT JOIN stock_lots l ON l.id=e.lot_id WHERE ($1::uuid IS NULL OR e.warehouse_id=$1) AND ($2::uuid IS NULL OR e.product_id=$2)").bind(filter.warehouse_id).bind(filter.product_id).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}
