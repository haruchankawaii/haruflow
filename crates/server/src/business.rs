use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    domain,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/customers", get(customers).post(create_customer))
        .route("/products", get(products).post(create_product))
        .route("/inventory/receipts", post(receive_stock))
        .route("/inventory/movements", get(movements))
        .route("/orders", get(orders).post(create_order))
        .route("/orders/{id}", get(order_detail))
        .route("/orders/{id}/confirm", post(confirm_order))
        .route("/orders/{id}/ship", post(ship_order))
        .route("/orders/{id}/invoice", post(invoice_order))
        .route("/invoices", get(invoices))
        .route("/invoices/{id}/payments", post(pay_invoice))
        .route("/accounting/accounts", get(accounts))
        .route("/accounting/journals", get(journals))
        .route("/accounting/journals/{id}", get(journal_detail))
        .route("/accounting/trial-balance", get(trial_balance))
}

pub(crate) fn parse(input: &str, scale: u32, label: &str) -> AppResult<Decimal> {
    domain::decimal(input, scale, label).map_err(AppError::bad_request)
}

pub(crate) fn text(input: &str, max: usize, label: &str) -> AppResult<String> {
    let value = input.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(AppError::bad_request(format!(
            "{label} must contain 1–{max} characters"
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(input: Option<String>, max: usize, label: &str) -> AppResult<Option<String>> {
    input
        .filter(|s| !s.trim().is_empty())
        .map(|s| text(&s, max, label))
        .transpose()
}

pub(crate) fn payload<T: Serialize>(operation: &str, user: &AuthUser, request: &T) -> Value {
    json!({"operation":operation,"actor_id":user.id,"request":request})
}

pub(crate) async fn cached(
    tx: &mut Transaction<'_, Postgres>,
    key: Uuid,
    request: &Value,
) -> AppResult<Option<Value>> {
    // The transaction lock covers both the first request and concurrent retries.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key.to_string())
        .execute(&mut **tx)
        .await?;
    let row = sqlx::query("SELECT response,request_hash=md5($2::jsonb::text) AS matches FROM idempotency_keys WHERE key=$1")
        .bind(key).bind(request).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        if !row.get::<bool, _>("matches") {
            return Err(AppError::conflict(
                "Idempotency key was already used for another request",
            ));
        }
        return Ok(Some(row.get("response")));
    }
    Ok(None)
}

pub(crate) async fn remember(
    tx: &mut Transaction<'_, Postgres>,
    key: Uuid,
    request: &Value,
    response: &Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO idempotency_keys(key,request_hash,response) VALUES($1,md5($2::jsonb::text),$3)")
        .bind(key).bind(request).bind(response).execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    action: &str,
    kind: &str,
    id: Uuid,
    details: Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO audit_events(id,actor_id,action,resource_type,resource_id,details) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(user.id).bind(action).bind(kind).bind(id).bind(details)
        .execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn journal(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    source: &str,
    source_id: Uuid,
    description: &str,
    accounts: (&str, &str),
    amount: Decimal,
) -> AppResult<()> {
    let (debit_account, credit_account) = accounts;
    crate::corrections::check_open(tx).await?;
    if amount == Decimal::ZERO {
        return Ok(());
    }
    if amount < Decimal::ZERO || amount.scale() > 2 || debit_account == credit_account {
        return Err(AppError::bad_request("Invalid balanced journal posting"));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,$3,$4,$5)")
        .bind(id).bind(description).bind(source).bind(source_id).bind(user.id).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,0),($5,$2,$6,0,$4)")
        .bind(Uuid::new_v4()).bind(id).bind(debit_account).bind(amount).bind(Uuid::new_v4()).bind(credit_account)
        .execute(&mut **tx).await?;
    Ok(())
}

async fn dashboard(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(r#"
        SELECT jsonb_build_object(
            'revenue',COALESCE((SELECT sum(credit-debit) FROM journal_lines WHERE account_code='4000'),0)::text,
            'outstanding',COALESCE((SELECT sum(total-paid) FROM invoices WHERE status<>'void'),0)::text,
            'inventory_value',COALESCE((SELECT sum(inventory_value) FROM products),0)::text,
            'cash',COALESCE((SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1100'),0)::text,
            'orders_count',(SELECT count(*) FROM sales_orders),
            'products_count',(SELECT count(*) FROM products),
            'customers_count',(SELECT count(*) FROM customers),
            'recent_orders',COALESCE((SELECT jsonb_agg(x.item ORDER BY x.created_at DESC) FROM (
                SELECT o.created_at,jsonb_build_object('id',o.id,'number',o.number,'customer_id',o.customer_id,
                'customer_name',c.name,'status',o.status,'total',o.total::text,'paid',COALESCE(i.paid,0)::text,'created_at',o.created_at,
                'has_stock_items',EXISTS(SELECT 1 FROM sales_order_lines sl WHERE sl.order_id=o.id AND sl.product_kind='stock')) item
                FROM sales_orders o JOIN customers c ON c.id=o.customer_id LEFT JOIN invoices i ON i.order_id=o.id AND i.status<>'void'
                ORDER BY o.created_at DESC LIMIT 6) x),'[]'::jsonb),
            'monthly_sales',COALESCE((SELECT jsonb_agg(jsonb_build_object('month',x.sales_month,'total',x.total::text) ORDER BY x.sales_month) FROM (
                SELECT to_char(created_at AT TIME ZONE (SELECT timezone FROM company),'YYYY-MM') AS sales_month,sum(total) total FROM invoices
                WHERE status<>'void' AND created_at>=(date_trunc('month',now() AT TIME ZONE (SELECT timezone FROM company))-interval '5 months') AT TIME ZONE (SELECT timezone FROM company) GROUP BY 1) x),'[]'::jsonb))
    "#).fetch_one(&state.db).await?;
    Ok(Json(result))
}

async fn customers(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY c.created_at DESC),'[]'::jsonb) FROM customers c")
        .fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize)]
struct CustomerInput {
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
}

async fn create_customer(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CustomerInput>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let name = text(&body.name, 200, "Name")?;
    let email = optional_text(body.email, 254, "Email")?;
    let phone = optional_text(body.phone, 64, "Phone")?;
    let address = optional_text(body.address, 1000, "Address")?;
    let id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    let result: Value = sqlx::query_scalar("INSERT INTO customers(id,name,email,phone,address) VALUES($1,$2,$3,$4,$5) RETURNING to_jsonb(customers)")
        .bind(id).bind(name).bind(email).bind(phone).bind(address).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "customer.created",
        "customer",
        id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn products(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'id',id,'sku',sku,'name',name,'kind',kind,'unit',unit,'price',price::text,'quantity',quantity::text,
        'average_cost',average_cost::text,'inventory_value',inventory_value::text,'created_at',created_at) ORDER BY name),'[]'::jsonb) FROM products"#)
        .fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize)]
struct ProductInput {
    sku: String,
    name: String,
    kind: Option<String>,
    unit: Option<String>,
    price: String,
}

pub(crate) fn product_kind(value: &str) -> AppResult<&str> {
    match value {
        "stock" | "service" => Ok(value),
        _ => Err(AppError::bad_request(
            "Product kind must be stock or service",
        )),
    }
}

async fn create_product(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ProductInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let sku = text(&body.sku.trim().to_uppercase(), 64, "SKU")?;
    let name = text(&body.name, 200, "Name")?;
    let kind = product_kind(body.kind.as_deref().unwrap_or("stock"))?;
    let unit = text(body.unit.as_deref().unwrap_or("pcs"), 32, "Unit")?;
    let price = parse(&body.price, 2, "Price")?;
    let id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    let inserted =
        sqlx::query("INSERT INTO products(id,sku,name,unit,price,kind) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(id)
            .bind(&sku)
            .bind(&name)
            .bind(&unit)
            .bind(price)
            .bind(kind)
            .execute(&mut *tx)
            .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .and_then(|e| e.code())
            .is_some_and(|code| code == "23505")
        {
            return Err(AppError::conflict("SKU already exists"));
        }
        return Err(error.into());
    }
    audit(
        &mut tx,
        &user,
        "product.created",
        "product",
        id,
        json!({"sku":sku,"kind":kind}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"sku":sku,"name":name,"kind":kind,"unit":unit,"price":price.to_string(),"quantity":"0.000","average_cost":"0.0000","inventory_value":"0.00"}),
    ))
}

#[derive(Deserialize, Serialize)]
struct ReceiptInput {
    product_id: Uuid,
    quantity: String,
    unit_cost: String,
    reference: Option<String>,
    idempotency_key: Uuid,
}

async fn receive_stock(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ReceiptInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("inventory.receipt", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    crate::corrections::check_open(&mut tx).await?;
    let quantity = parse(&body.quantity, 3, "Quantity")?;
    let unit_cost = parse(&body.unit_cost, 4, "Unit cost")?;
    let reference = optional_text(body.reference, 200, "Reference")?;
    let value = domain::money(quantity * unit_cost);
    if value <= Decimal::ZERO {
        return Err(AppError::bad_request("Receipt value must be at least 0.01"));
    }
    if value > Decimal::from(1_000_000_000_000u64) {
        return Err(AppError::bad_request("Receipt value limit exceeded"));
    }
    let product = sqlx::query(
        "SELECT kind,quantity,average_cost,inventory_value FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(body.product_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::bad_request("Product not found"))?;
    if product.get::<String, _>("kind") != "stock" {
        return Err(AppError::bad_request("Services cannot receive stock"));
    }
    let new_quantity = product.get::<Decimal, _>("quantity") + quantity;
    if new_quantity > Decimal::from(1_000_000_000u64) {
        return Err(AppError::bad_request("Stock quantity limit exceeded"));
    }
    let new_value = product.get::<Decimal, _>("inventory_value") + value;
    if new_value > Decimal::from(1_000_000_000_000u64) {
        return Err(AppError::bad_request(
            "Product inventory value limit exceeded",
        ));
    }
    let new_average = domain::average(new_value, new_quantity);
    sqlx::query("UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1")
        .bind(body.product_id)
        .bind(new_quantity)
        .bind(new_value)
        .bind(new_average)
        .execute(&mut *tx)
        .await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,before_quantity,before_average_cost,before_inventory_value) VALUES($1,$2,'receipt',$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(id).bind(body.product_id).bind(quantity).bind(unit_cost).bind(value).bind(reference).bind(user.id)
        .bind(product.get::<Decimal,_>("quantity")).bind(product.get::<Decimal,_>("average_cost"))
        .bind(product.get::<Decimal,_>("inventory_value")).execute(&mut *tx).await?;
    journal(
        &mut tx,
        &user,
        "receipt",
        id,
        "Opening stock / inventory receipt",
        ("1300", "3000"),
        value,
    )
    .await?;
    audit(
        &mut tx,
        &user,
        "inventory.received",
        "inventory_movement",
        id,
        json!({"product_id":body.product_id,"quantity":quantity.to_string()}),
    )
    .await?;
    let result = json!({"id":id,"product_id":body.product_id,"quantity":quantity.to_string(),"value":value.to_string()});
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn movements(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(x.item ORDER BY x.movement_seq DESC),'[]'::jsonb) FROM (
        SELECT m.movement_seq,jsonb_build_object('id',m.id,'product_id',m.product_id,'sku',p.sku,'product_name',p.name,
            'kind',m.kind,'quantity',m.quantity::text,'unit_cost',m.unit_cost::text,'value',m.value::text,
            'reference',m.reference,'order_id',m.order_id,'created_at',m.created_at,'reverses_movement_id',m.reverses_id,
            'purchase_receipt_id',m.purchase_receipt_id,'purchase_order_id',pr.order_id,
            'purchase_order_number',po.number,
            'can_reverse',m.kind='receipt' AND r.id IS NULL AND m.before_quantity IS NOT NULL AND NOT EXISTS(
                SELECT 1 FROM inventory_movements later WHERE later.product_id=m.product_id AND later.movement_seq>m.movement_seq)
                AND (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton),
            'reversal_blocked_reason',CASE WHEN m.kind<>'receipt' THEN NULL
                WHEN r.id IS NOT NULL THEN 'Penerimaan sudah dibalik.'
                WHEN m.before_quantity IS NULL THEN 'Penerimaan sebelum upgrade belum memiliki snapshot biaya; gunakan penyesuaian stok.'
                WHEN EXISTS(SELECT 1 FROM inventory_movements later WHERE later.product_id=m.product_id AND later.movement_seq>m.movement_seq)
                    THEN 'Ada mutasi stok setelah penerimaan ini; gunakan penyesuaian stok untuk menjaga HPP historis.'
                WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton)
                    THEN 'Periode akuntansi saat ini sudah ditutup.' ELSE NULL END,
            'reversal',CASE WHEN r.id IS NULL THEN NULL ELSE jsonb_build_object('id',r.id,'reason',r.reason,'created_at',r.created_at,'journal_id',rj.id) END) item
        FROM inventory_movements m JOIN products p ON p.id=m.product_id
        LEFT JOIN posting_reversals r ON (r.source_type='receipt' AND r.source_id=m.id AND m.kind='receipt')
            OR (r.source_type='purchase_receipt' AND r.source_id=m.purchase_receipt_id AND m.kind='purchase_receipt')
        LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=r.id
        LEFT JOIN purchase_receipts pr ON pr.id=m.purchase_receipt_id
        LEFT JOIN purchase_orders po ON po.id=pr.order_id
        ORDER BY m.movement_seq DESC LIMIT 500) x"#)
        .fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

pub(crate) const ORDER_SELECT: &str = r#"SELECT jsonb_build_object('id',o.id,'number',o.number,'customer_id',o.customer_id,
    'customer_name',c.name,'status',o.status,'total',o.total::text,'paid',COALESCE(i.paid,0)::text,
    'has_stock_items',EXISTS(SELECT 1 FROM sales_order_lines sl WHERE sl.order_id=o.id AND sl.product_kind='stock'),
    'invoice_id',i.id,'created_at',o.created_at,'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
    'id',l.id,'product_id',l.product_id,'sku',l.product_sku,'name',l.product_name,'product_kind',l.product_kind,'quantity',l.quantity::text,
    'unit_price',l.unit_price::text,'total',l.total::text) ORDER BY l.product_sku) FROM sales_order_lines l WHERE l.order_id=o.id),'[]'::jsonb))
    FROM sales_orders o JOIN customers c ON c.id=o.customer_id LEFT JOIN invoices i ON i.order_id=o.id AND i.status<>'void' WHERE o.id=$1"#;

async fn orders(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(x.item ORDER BY x.created_at DESC),'[]'::jsonb) FROM (
        SELECT o.created_at,jsonb_build_object('id',o.id,'number',o.number,'customer_id',o.customer_id,'customer_name',c.name,
        'status',o.status,'total',o.total::text,'paid',COALESCE(i.paid,0)::text,'invoice_id',i.id,'created_at',o.created_at,
        'has_stock_items',EXISTS(SELECT 1 FROM sales_order_lines sl WHERE sl.order_id=o.id AND sl.product_kind='stock')) item
        FROM sales_orders o JOIN customers c ON c.id=o.customer_id LEFT JOIN invoices i ON i.order_id=o.id AND i.status<>'void'
        ORDER BY o.created_at DESC LIMIT 500) x"#).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

async fn order_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::bad_request("Order not found"))?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
struct OrderLineInput {
    product_id: Uuid,
    quantity: String,
    unit_price: String,
}
#[derive(Deserialize, Serialize)]
struct OrderInput {
    customer_id: Uuid,
    items: Vec<OrderLineInput>,
    idempotency_key: Uuid,
}

async fn create_order(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<OrderInput>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let request = payload("order.create", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    if body.items.is_empty() || body.items.len() > 100 {
        return Err(AppError::bad_request("Order must contain 1–100 items"));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customers WHERE id=$1)")
        .bind(body.customer_id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::bad_request("Customer not found"));
    }
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    let mut total = Decimal::ZERO;
    // Lock in a stable order so product edits cannot change kind during snapshotting.
    let product_ids: Vec<Uuid> = body.items.iter().map(|line| line.product_id).collect();
    sqlx::query("SELECT id FROM products WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&product_ids)
        .fetch_all(&mut *tx)
        .await?;
    for line in &body.items {
        if !seen.insert(line.product_id) {
            return Err(AppError::bad_request(
                "Combine duplicate products into one order line",
            ));
        }
        let quantity = parse(&line.quantity, 3, "Quantity")?;
        let price = parse(&line.unit_price, 2, "Unit price")?;
        let line_total = domain::money(quantity * price);
        if line_total <= Decimal::ZERO {
            return Err(AppError::bad_request(
                "Each order line total must be at least 0.01",
            ));
        }
        total += line_total;
        if total > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request("Order value limit exceeded"));
        }
        let product = sqlx::query("SELECT sku,name,kind FROM products WHERE id=$1")
            .bind(line.product_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Product not found"))?;
        lines.push((
            line.product_id,
            product.get::<String, _>("sku"),
            product.get::<String, _>("name"),
            product.get::<String, _>("kind"),
            quantity,
            price,
            line_total,
        ));
    }
    let id = Uuid::new_v4();
    let number: String = sqlx::query_scalar(
        "SELECT order_prefix||'-'||lpad(nextval('sales_order_number')::text,6,'0') FROM company",
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO sales_orders(id,number,customer_id,total,created_by) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(number)
    .bind(body.customer_id)
    .bind(total)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    for (product_id, sku, name, kind, quantity, price, line_total) in lines {
        sqlx::query("INSERT INTO sales_order_lines(id,order_id,product_id,product_sku,product_name,product_kind,quantity,unit_price,total) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(Uuid::new_v4()).bind(id).bind(product_id).bind(sku).bind(name).bind(kind).bind(quantity).bind(price).bind(line_total)
            .execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        &user,
        "order.created",
        "sales_order",
        id,
        json!({"total":total.to_string()}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize, Serialize)]
struct ActionInput {
    idempotency_key: Uuid,
}

async fn confirm_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionInput>,
) -> AppResult<Json<Value>> {
    user.require(&["sales"])?;
    let request = payload("order.confirm", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    let status: String =
        sqlx::query_scalar("SELECT status FROM sales_orders WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Order not found"))?;
    if status != "draft" {
        return Err(AppError::conflict("Only draft orders can be confirmed"));
    }
    sqlx::query("UPDATE sales_orders SET status='confirmed',confirmed_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "order.confirmed",
        "sales_order",
        id,
        json!({}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn ship_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionInput>,
) -> AppResult<Json<Value>> {
    user.require(&["warehouse"])?;
    let request = payload("order.ship", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    crate::corrections::check_open(&mut tx).await?;
    let order = sqlx::query("SELECT status,number FROM sales_orders WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::bad_request("Order not found"))?;
    if order.get::<String, _>("status") != "confirmed" {
        return Err(AppError::conflict("Only confirmed orders can be shipped"));
    }
    // Services are fulfilled without an inventory posting; mixed orders move stock only.
    let lines = sqlx::query(
        "SELECT product_id,quantity FROM sales_order_lines WHERE order_id=$1 AND product_kind='stock' ORDER BY product_id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    if lines.is_empty() {
        return Err(AppError::conflict(
            "Service-only orders can be invoiced directly without shipment",
        ));
    }
    let posting_id = Uuid::new_v4();
    sqlx::query("INSERT INTO shipment_postings(id,order_id) VALUES($1,$2)")
        .bind(posting_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // Every shipment locks products in the same order, avoiding cross-order deadlocks.
    let mut cost_total = Decimal::ZERO;
    for line in lines {
        let product_id: Uuid = line.get("product_id");
        let quantity: Decimal = line.get("quantity");
        let product = sqlx::query(
            "SELECT sku,quantity,average_cost,inventory_value FROM products WHERE id=$1 FOR UPDATE",
        )
        .bind(product_id)
        .fetch_one(&mut *tx)
        .await?;
        let on_hand: Decimal = product.get("quantity");
        if on_hand < quantity {
            return Err(AppError::conflict(format!(
                "Insufficient stock for {}",
                product.get::<String, _>("sku")
            )));
        }
        let average: Decimal = product.get("average_cost");
        let value: Decimal = product.get("inventory_value");
        let cost = domain::shipment_cost(quantity, on_hand, average, value);
        let remaining = on_hand - quantity;
        let remaining_value = value - cost;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(product_id)
        .bind(remaining)
        .bind(remaining_value)
        .bind(if remaining == Decimal::ZERO {
            Decimal::ZERO
        } else {
            average
        })
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,order_id,created_by,shipment_posting_id) VALUES($1,$2,'shipment',$3,$4,$5,$6,$7,$8,$9)")
            .bind(Uuid::new_v4()).bind(product_id).bind(-quantity).bind(average).bind(cost)
            .bind(order.get::<String,_>("number")).bind(id).bind(user.id).bind(posting_id).execute(&mut *tx).await?;
        cost_total += cost;
    }
    journal(
        &mut tx,
        &user,
        "shipment",
        posting_id,
        &format!("Shipment {}", order.get::<String, _>("number")),
        ("5000", "1300"),
        cost_total,
    )
    .await?;
    sqlx::query("UPDATE sales_orders SET status='shipped',shipped_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "order.shipped",
        "sales_order",
        id,
        json!({"cost":cost_total.to_string()}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(ORDER_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn invoice_order(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("order.invoice", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    crate::corrections::check_open(&mut tx).await?;
    let order =
        sqlx::query("SELECT status,total,customer_id FROM sales_orders WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Order not found"))?;
    let has_stock_items: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sales_order_lines WHERE order_id=$1 AND product_kind='stock')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let status: String = order.get("status");
    if status != "shipped" && !(status == "confirmed" && !has_stock_items) {
        return Err(AppError::conflict(
            "Confirm service orders or ship stock orders before invoicing",
        ));
    }
    let invoice_id = Uuid::new_v4();
    let number: String = sqlx::query_scalar(
        "SELECT invoice_prefix||'-'||lpad(nextval('invoice_number')::text,6,'0') FROM company",
    )
    .fetch_one(&mut *tx)
    .await?;
    let total: Decimal = order.get("total");
    sqlx::query(
        "INSERT INTO invoices(id,number,order_id,customer_id,total) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(invoice_id)
    .bind(&number)
    .bind(id)
    .bind(order.get::<Uuid, _>("customer_id"))
    .bind(total)
    .execute(&mut *tx)
    .await?;
    journal(
        &mut tx,
        &user,
        "invoice",
        invoice_id,
        &format!("Invoice {number}"),
        ("1200", "4000"),
        total,
    )
    .await?;
    sqlx::query("UPDATE sales_orders SET status='invoiced',invoiced_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &user,
        "invoice.posted",
        "invoice",
        invoice_id,
        json!({"order_id":id,"total":total.to_string()}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(invoice_id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

pub(crate) const INVOICE_SELECT: &str = r#"SELECT jsonb_build_object('id',i.id,'number',i.number,'order_id',i.order_id,
    'customer_id',i.customer_id,'customer_name',c.name,'status',i.status,'total',i.total::text,'paid',i.paid::text,
    'outstanding',(CASE WHEN i.status='void' THEN 0 ELSE i.total-i.paid END)::text,'created_at',i.created_at)
    FROM invoices i JOIN customers c ON c.id=i.customer_id WHERE i.id=$1"#;

async fn invoices(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(x.item ORDER BY x.created_at DESC),'[]'::jsonb) FROM (
        SELECT i.created_at,jsonb_build_object('id',i.id,'number',i.number,'order_id',i.order_id,'customer_id',i.customer_id,
        'customer_name',c.name,'status',i.status,'total',i.total::text,'paid',i.paid::text,'outstanding',(CASE WHEN i.status='void' THEN 0 ELSE i.total-i.paid END)::text,'created_at',i.created_at) item
        FROM invoices i JOIN customers c ON c.id=i.customer_id ORDER BY i.created_at DESC LIMIT 500) x"#).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize, Serialize)]
struct PaymentInput {
    amount: String,
    reference: Option<String>,
    idempotency_key: Uuid,
}

async fn pay_invoice(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PaymentInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("invoice.payment", &user, &json!({"id":id,"body":body}));
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    crate::corrections::check_open(&mut tx).await?;
    let amount = parse(&body.amount, 2, "Amount")?;
    let reference = optional_text(body.reference, 200, "Reference")?;
    let invoice =
        sqlx::query("SELECT total,paid,number,status FROM invoices WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Invoice not found"))?;
    if invoice.get::<String, _>("status") == "void" {
        return Err(AppError::conflict(
            "A reversed invoice cannot receive payments",
        ));
    }
    let total: Decimal = invoice.get("total");
    let paid: Decimal = invoice.get("paid");
    if amount > total - paid {
        return Err(AppError::conflict(
            "Payment exceeds outstanding invoice balance",
        ));
    }
    let payment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO payments(id,invoice_id,amount,reference,created_by) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(payment_id)
    .bind(id)
    .bind(amount)
    .bind(reference)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let new_paid = paid + amount;
    sqlx::query("UPDATE invoices SET paid=$2,status=$3 WHERE id=$1")
        .bind(id)
        .bind(new_paid)
        .bind(if new_paid == total { "paid" } else { "partial" })
        .execute(&mut *tx)
        .await?;
    journal(
        &mut tx,
        &user,
        "payment",
        payment_id,
        &format!("Payment for {}", invoice.get::<String, _>("number")),
        ("1100", "1200"),
        amount,
    )
    .await?;
    audit(
        &mut tx,
        &user,
        "invoice.payment_received",
        "payment",
        payment_id,
        json!({"invoice_id":id,"amount":amount.to_string()}),
    )
    .await?;
    let result: Value = sqlx::query_scalar(INVOICE_SELECT)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn accounts(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let items: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(a)||jsonb_build_object('is_control',a.code IN ('1200','1300') OR a.system_role IS NOT NULL) ORDER BY code),'[]'::jsonb) FROM accounts a",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({"items":items})))
}

const JOURNAL_ROWS: &str = r#"        SELECT j.id,j.created_at,jsonb_build_object('id',j.id,'description',j.description,'source_type',j.source_type,
        'source_id',j.source_id,'created_at',j.created_at,
        'can_reverse',j.source_type='manual' AND r.id IS NULL AND (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton),
        'reversal_blocked_reason',CASE WHEN j.source_type<>'manual' THEN NULL
            WHEN r.id IS NOT NULL THEN 'Jurnal sudah dibalik.'
            WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton)
                THEN 'Periode akuntansi saat ini sudah ditutup.' ELSE NULL END,
        'reversal',CASE WHEN r.id IS NULL THEN NULL ELSE jsonb_build_object('id',r.id,'reason',r.reason,'created_at',r.created_at,'journal_id',rj.id) END,
        'reversed_source',CASE WHEN original.id IS NULL THEN NULL ELSE jsonb_build_object('source_type',original.source_type,'source_id',original.source_id) END,
        'lines',(SELECT jsonb_agg(jsonb_build_object(
        'account_code',l.account_code,'account_name',a.name,'debit',l.debit::text,'credit',l.credit::text) ORDER BY l.debit DESC,l.account_code)
        FROM journal_lines l JOIN accounts a ON a.code=l.account_code WHERE l.journal_id=j.id)) item
        FROM journal_entries j
        LEFT JOIN posting_reversals r ON r.source_type='manual' AND r.source_id=j.id AND j.source_type='manual'
        LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=r.id
        LEFT JOIN posting_reversals original ON j.source_type='reversal' AND original.id=j.source_id
"#;

async fn journals(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let sql = format!(
        "SELECT COALESCE(jsonb_agg(x.item ORDER BY x.created_at DESC,x.id DESC),'[]'::jsonb) FROM ({JOURNAL_ROWS} ORDER BY j.created_at DESC,j.id DESC LIMIT 500) x"
    );
    let items: Value = sqlx::query_scalar(&sql).fetch_one(&state.db).await?;
    Ok(Json(json!({"items":items})))
}

async fn journal_detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let sql = format!("SELECT item FROM ({JOURNAL_ROWS} WHERE j.id=$1) x");
    let item: Value = sqlx::query_scalar(&sql)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(AppError::not_found)?;
    Ok(Json(item))
}

async fn trial_balance(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let result: Value = sqlx::query_scalar(r#"WITH net_balances AS (
        SELECT a.code,a.name,a.kind,COALESCE(sum(l.debit-l.credit),0) balance
        FROM accounts a LEFT JOIN journal_lines l ON l.account_code=a.code GROUP BY a.code,a.name,a.kind),
        balances AS (SELECT code,name,kind,balance,greatest(balance,0) debit,greatest(-balance,0) credit FROM net_balances)
        SELECT jsonb_build_object('items',jsonb_agg(jsonb_build_object('code',code,'name',name,'kind',kind,
        'debit',debit::text,'credit',credit::text,'balance',balance::text) ORDER BY code),
        'total_debit',sum(debit)::text,'total_credit',sum(credit)::text,'balanced',sum(debit)=sum(credit)) FROM balances"#)
        .fetch_one(&state.db).await?;
    Ok(Json(result))
}
