use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::borrow::Cow;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    app: &Router,
    cookie: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(if body.is_some() { "POST" } else { "GET" })
                .uri(path)
                .header("cookie", cookie)
                .header("x-haruflow-request", "1")
                .header("content-type", "application/json")
                .body(body.map(|v| Body::from(v.to_string())).unwrap_or_default())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, value)
}
async fn call(app: &Router, cookie: &str, path: &str, body: Option<Value>) -> Value {
    let (status, value) = request(app, cookie, path, body).await;
    assert!(status.is_success(), "{path}: {status}: {value}");
    value
}
async fn actor(pool: &PgPool, role: &str) -> String {
    let user = Uuid::new_v4();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Partial sales tester',$2,'unused',$3)").bind(user).bind(format!("{user}@example.test")).bind(role).execute(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '1 day')",
    )
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
    format!("hf_session={token}")
}
async fn fixture(pool: &PgPool) -> (Router, String, Uuid, Uuid, Uuid) {
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Partial sales')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let cookie = actor(pool, "admin").await;
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "partial-sales-fixture-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        ".local/no-static".into(),
    );
    let customer = call(
        &app,
        &cookie,
        "/api/customers",
        Some(json!({"name":"Customer"})),
    )
    .await;
    let stock = call(
        &app,
        &cookie,
        "/api/products",
        Some(json!({"sku":"PART-STOCK","name":"Stock","price":"5"})),
    )
    .await;
    let service=call(&app,&cookie,"/api/products",Some(json!({"sku":"PART-SERVICE","name":"Service","kind":"service","unit":"hour","price":"5"}))).await;
    call(&app,&cookie,"/api/inventory/receipts",Some(json!({"product_id":stock["id"],"quantity":"3","unit_cost":"3.3333","idempotency_key":Uuid::new_v4()}))).await;
    (app, cookie, uid(&customer), uid(&stock), uid(&service))
}
fn uid(value: &Value) -> Uuid {
    value["id"].as_str().unwrap().parse().unwrap()
}
fn decimal(value: &Value) -> Decimal {
    value.as_str().unwrap().parse().unwrap()
}
async fn order(app: &Router, cookie: &str, customer: Uuid, items: Value) -> Value {
    let order = call(
        app,
        cookie,
        "/api/orders",
        Some(json!({"customer_id":customer,"items":items,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    call(
        app,
        cookie,
        &format!("/api/orders/{}/confirm", uid(&order)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
fn allocation(order: &Value, sku: &str, quantity: &str) -> Value {
    let line = order["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["sku"] == sku)
        .unwrap();
    json!({"items":[{"line_id":line["id"],"quantity":quantity}],"idempotency_key":Uuid::new_v4()})
}
fn reverse() -> Value {
    json!({"reason":"Correction for partial sales test","idempotency_key":Uuid::new_v4()})
}

#[sqlx::test(migrations = "../../migrations")]
async fn partial_deliveries_and_invoices_preserve_final_money_and_cost(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let order = order(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"3","unit_price":"5"}]),
    )
    .await;
    let id = uid(&order);
    let mut invoices = Vec::new();
    for step in 0..3 {
        let shipped = call(
            &app,
            &cookie,
            &format!("/api/orders/{id}/ship"),
            Some(allocation(&order, "PART-STOCK", "1")),
        )
        .await;
        assert_eq!(
            decimal(&shipped["items"][0]["shipped_quantity"]),
            Decimal::from(step + 1)
        );
        let body = allocation(&order, "PART-STOCK", "1");
        let invoice = call(
            &app,
            &cookie,
            &format!("/api/orders/{id}/invoice"),
            Some(body.clone()),
        )
        .await;
        assert_eq!(
            invoice,
            call(
                &app,
                &cookie,
                &format!("/api/orders/{id}/invoice"),
                Some(body)
            )
            .await
        );
        assert_eq!(decimal(&invoice["total"]), Decimal::from(5));
        invoices.push(invoice);
    }
    let detail = call(&app, &cookie, &format!("/api/orders/{id}"), None).await;
    assert_eq!(detail["status"], "invoiced");
    assert_eq!(detail["shipments"].as_array().unwrap().len(), 3);
    assert_eq!(detail["invoices"].as_array().unwrap().len(), 3);
    assert_eq!(detail["can_ship"], false);
    assert_eq!(detail["can_invoice"], false);
    let total: Decimal = sqlx::query_scalar(
        "SELECT sum(value) FROM inventory_movements WHERE order_id=$1 AND kind='shipment'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, Decimal::from(10));
    let row = sqlx::query("SELECT quantity,inventory_value FROM products WHERE id=$1")
        .bind(stock)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Decimal, _>("quantity"), Decimal::ZERO);
    assert_eq!(row.get::<Decimal, _>("inventory_value"), Decimal::ZERO);
    let list = call(&app, &cookie, "/api/orders", None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let dash = call(&app, &cookie, "/api/dashboard", None).await;
    assert_eq!(dash["recent_orders"].as_array().unwrap().len(), 1);
    let journal: Decimal = sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(journal, Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations")]
async fn partial_invoice_rounding_and_service_fulfillment_are_independent(pool: PgPool) {
    let (app, cookie, customer, stock, service) = fixture(&pool).await;
    let order=order(&app,&cookie,customer,json!([{"product_id":stock,"quantity":"1","unit_price":"5"},{"product_id":service,"quantity":"0.003","unit_price":"5"}])).await;
    let id = uid(&order);
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let first = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(allocation(&order, "PART-SERVICE", "0.001")),
    )
    .await;
    assert_eq!(first["total"], "0.01");
    let second = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(allocation(&order, "PART-SERVICE", "0.002")),
    )
    .await;
    assert_eq!(second["total"], "0.01");
    let shipping = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/ship"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(shipping["status"], "shipped");
    let final_invoice = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(final_invoice["total"], "5.00");
    assert_eq!(final_invoice["items"].as_array().unwrap().len(), 1);
    let value: Decimal =
        sqlx::query_scalar("SELECT sum(total) FROM invoices WHERE order_id=$1 AND status<>'void'")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(value, Decimal::new(502, 2));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_partial_requests_cannot_overdeliver_or_overinvoice(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let order = order(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"3","unit_price":"5"}]),
    )
    .await;
    let id = uid(&order);
    let ship = format!("/api/orders/{id}/ship");
    let (a, b) = tokio::join!(
        request(
            &app,
            &cookie,
            &ship,
            Some(allocation(&order, "PART-STOCK", "2"))
        ),
        request(
            &app,
            &cookie,
            &ship,
            Some(allocation(&order, "PART-STOCK", "2"))
        )
    );
    assert_eq!([a.0, b.0].iter().filter(|s| s.is_success()).count(), 1);
    assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
    let invoice = format!("/api/orders/{id}/invoice");
    let (a, b) = tokio::join!(
        request(
            &app,
            &cookie,
            &invoice,
            Some(allocation(&order, "PART-STOCK", "1.5"))
        ),
        request(
            &app,
            &cookie,
            &invoice,
            Some(allocation(&order, "PART-STOCK", "1.5"))
        )
    );
    assert_eq!([a.0, b.0].iter().filter(|s| s.is_success()).count(), 1);
    assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
    let detail = call(&app, &cookie, &format!("/api/orders/{id}"), None).await;
    assert_eq!(
        decimal(&detail["items"][0]["invoiceable_quantity"]),
        Decimal::new(5, 1)
    );
    let stale = allocation(&order, "PART-STOCK", "0.5");
    let mut stale = stale;
    stale["version"] = order["version"].clone();
    let (status, _) = request(&app, &cookie, &invoice, Some(stale)).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn shipment_reversal_checks_only_its_invoice_allocations(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let order = order(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"3","unit_price":"5"}]),
    )
    .await;
    let id = uid(&order);
    let first = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/ship"),
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    let first_id = first["shipments"][0]["id"].as_str().unwrap();
    let invoice = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    let second = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/ship"),
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    let second_id = second["shipments"][1]["id"].as_str().unwrap();
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/orders/{id}/shipments/{first_id}/reverse"),
        Some(reverse()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let restored = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/shipments/{second_id}/reverse"),
        Some(reverse()),
    )
    .await;
    assert_eq!(
        decimal(&restored["items"][0]["shipped_quantity"]),
        Decimal::ONE
    );
    assert_eq!(restored["can_ship"], true);
    call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/reverse", uid(&invoice)),
        Some(reverse()),
    )
    .await;
    call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/shipments/{first_id}/reverse"),
        Some(reverse()),
    )
    .await;
    let qty: Decimal = sqlx::query_scalar("SELECT quantity FROM products WHERE id=$1")
        .bind(stock)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qty, Decimal::from(3));
    let reship = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/ship"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(reship["status"], "shipped");
    let replacement = call(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(replacement["total"], "15.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn partial_sales_guards_roles_periods_atomic_validation_and_immutability(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let order = order(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"3","unit_price":"5"}]),
    )
    .await;
    let id = uid(&order);
    let path = format!("/api/orders/{id}/ship");
    for role in ["sales", "finance", "viewer"] {
        let actor = actor(&pool, role).await;
        let (status, _) = request(
            &app,
            &actor,
            &path,
            Some(allocation(&order, "PART-STOCK", "1")),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    for input in [
        json!({"items":[],"idempotency_key":Uuid::new_v4()}),
        json!({"items":[{"line_id":Uuid::new_v4(),"quantity":"1"}],"idempotency_key":Uuid::new_v4()}),
        allocation(&order, "PART-STOCK", "0"),
        allocation(&order, "PART-STOCK", "0.0001"),
    ] {
        let (status, _) = request(&app, &cookie, &path, Some(input)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let shipment = call(
        &app,
        &cookie,
        &path,
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    assert!(
        sqlx::query("UPDATE shipment_allocations SET quantity=2")
            .execute(&pool)
            .await
            .is_err()
    );
    let line = order["items"][0]["id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let shipment_id = shipment["shipments"][0]["id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let movement: Uuid = sqlx::query_scalar("SELECT movement_id FROM shipment_allocations LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO shipment_allocations(id,shipment_id,line_id,quantity,movement_id) VALUES($1,$2,$3,1,$4)").bind(Uuid::new_v4()).bind(shipment_id).bind(line).bind(movement).execute(&pool).await.is_err());
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(allocation(&order, "PART-STOCK", "1")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM shipment_postings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tiny_invoice_remainder_cannot_be_stranded_by_rounding(pool: PgPool) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let order = order(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"1","unit_price":"0.01"}]),
    )
    .await;
    let path = format!("/api/orders/{}/invoice", uid(&order));
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(allocation(&order, "PART-SERVICE", "0.999")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let invoice = call(
        &app,
        &cookie,
        &path,
        Some(allocation(&order, "PART-SERVICE", "1")),
    )
    .await;
    assert_eq!(invoice["total"], "0.01");
    assert_eq!(invoice["items"][0]["quantity"], "1.000");
}

#[sqlx::test(migrations = false)]
async fn migration_backfills_legacy_allocations_without_changing_postings(pool: PgPool) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|m| m.version <= 13)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    let company = Uuid::new_v4();
    let user = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let product = Uuid::new_v4();
    let order = Uuid::new_v4();
    let line = Uuid::new_v4();
    let shipment = Uuid::new_v4();
    let movement = Uuid::new_v4();
    let invoice = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Legacy allocations')")
        .bind(company)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Admin','legacy-partial@example.test','unused','admin')").bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO customers(id,name) VALUES($1,'Customer')")
        .bind(customer)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO products(id,sku,name,price,quantity,average_cost,inventory_value) VALUES($1,'OLD','Old',5,2,3.3333,6.67)").bind(product).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO sales_orders(id,number,customer_id,status,total,created_by) VALUES($1,'SO-OLD',$2,'invoiced',5,$3)").bind(order).bind(customer).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO sales_order_lines(id,order_id,product_id,product_sku,product_name,quantity,unit_price,total) VALUES($1,$2,$3,'OLD','Old',1,5,5)").bind(line).bind(order).bind(product).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO shipment_postings(id,order_id) VALUES($1,$2)")
        .bind(shipment)
        .bind(order)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,order_id,created_by,shipment_posting_id) VALUES($1,$2,'shipment',-1,3.3333,3.33,$3,$4,$5)").bind(movement).bind(product).bind(order).bind(user).bind(shipment).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO invoices(id,number,order_id,customer_id,total,paid,status) VALUES($1,'INV-OLD',$2,$3,5,5,'paid')").bind(invoice).bind(order).bind(customer).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let snapshot:Value=sqlx::query_scalar("SELECT jsonb_build_object('products',(SELECT jsonb_agg(to_jsonb(p)) FROM products p),'movements',(SELECT jsonb_agg(to_jsonb(m)) FROM inventory_movements m),'journal_count',(SELECT count(*) FROM journal_entries))").fetch_one(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('products',(SELECT jsonb_agg(to_jsonb(p)) FROM products p),'movements',(SELECT jsonb_agg(to_jsonb(m)) FROM inventory_movements m),'journal_count',(SELECT count(*) FROM journal_entries))").fetch_one(&pool).await.unwrap();
    assert_eq!(snapshot, after);
    let doc: Value = sqlx::query_scalar("SELECT sales_order_document($1)")
        .bind(order)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(decimal(&doc["items"][0]["shipped_quantity"]), Decimal::ONE);
    assert_eq!(decimal(&doc["items"][0]["invoiced_quantity"]), Decimal::ONE);
    assert_eq!(doc["shipments"][0]["can_reverse"], false);
    let inv: Value = sqlx::query_scalar("SELECT sales_invoice_document($1)")
        .bind(invoice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(inv["status"], "paid");
    assert_eq!(inv["items"][0]["total"], "5.00");
}
