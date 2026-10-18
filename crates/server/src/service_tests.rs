use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{AppState, app_router};

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, Option<String>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-haruflow-request", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let body = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response.headers().get("set-cookie").map(|value| {
        value
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    });
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap();
    (status, value, cookie)
}

async fn call(app: &Router, cookie: &str, method: &str, path: &str, body: Value) -> Value {
    let (status, value, _) = request(app, method, path, Some(cookie), Some(body)).await;
    assert!(status.is_success(), "{method} {path}: {status}: {value}");
    value
}

async fn action(app: &Router, cookie: &str, path: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        path,
        json!({"idempotency_key":Uuid::new_v4()}),
    )
    .await
}

async fn reverse(app: &Router, cookie: &str, path: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        path,
        json!({"reason":"Correction for test","idempotency_key":Uuid::new_v4()}),
    )
    .await
}

async fn setup(pool: &PgPool) -> (Router, String, String) {
    let token = "service-product-test-token-at-least-32-characters";
    let app = app_router(
        AppState {
            db: pool.clone(),
            setup_token: token.into(),
            secure_cookies: false,
        },
        "apps/web/dist".into(),
    );
    let (status, value, cookie) = request(&app, "POST", "/api/setup", None,
        Some(json!({"token":token,"company_name":"Service Test","name":"Admin","email":"service@example.test","password":"test-password-very-strong"}))).await;
    assert!(status.is_success(), "{status}: {value}");
    let cookie = cookie.unwrap();
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        json!({"name":"Customer"}),
    )
    .await;
    (app, cookie, customer["id"].as_str().unwrap().to_owned())
}

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap()
}

fn amount(value: &Value, key: &str) -> Decimal {
    value[key].as_str().unwrap().parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_order_invoices_and_pays_without_inventory(pool: PgPool) {
    let (app, cookie, customer) = setup(&pool).await;
    let service = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({
            "sku":"CONSULT","name":"Consultation","kind":"service","unit":"hour","price":"100000"
        }),
    )
    .await;
    assert_eq!(service["kind"], "service");
    let service_id = id(&service);
    for (path, body) in [
        (
            "/api/inventory/receipts",
            json!({"product_id":service_id,"quantity":"1","unit_cost":"50000","idempotency_key":Uuid::new_v4()}),
        ),
        (
            "/api/inventory/adjustments",
            json!({"product_id":service_id,"quantity":"1","unit_cost":"50000","reason":"Stock check","idempotency_key":Uuid::new_v4()}),
        ),
    ] {
        let (status, _, _) = request(&app, "POST", path, Some(&cookie), Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let order = call(&app, &cookie, "POST", "/api/orders", json!({
        "customer_id":customer,"items":[{"product_id":service_id,"quantity":"1.5","unit_price":"100000"}],"idempotency_key":Uuid::new_v4()
    })).await;
    let order_id = id(&order);
    assert_eq!(order["has_stock_items"], false);
    assert_eq!(order["items"][0]["product_kind"], "service");
    let (status, _, _) = request(
        &app,
        "PATCH",
        &format!("/api/products/{service_id}"),
        Some(&cookie),
        Some(json!({
            "sku":"CONSULT","name":"Consultation","kind":"stock","unit":"hour","price":"100000"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let edited = call(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/products/{service_id}"),
        json!({
            "sku":"CONSULT-NEW","name":"Renamed consultation","unit":"hour","price":"110000"
        }),
    )
    .await;
    assert_eq!(
        edited["kind"], "service",
        "omitting kind keeps its existing value"
    );
    action(&app, &cookie, &format!("/api/orders/{order_id}/confirm")).await;
    let (status, _, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order_id}/ship"),
        Some(&cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let invoice_request = json!({"idempotency_key":Uuid::new_v4()});
    let invoice_path = format!("/api/orders/{order_id}/invoice");
    let invoice = call(
        &app,
        &cookie,
        "POST",
        &invoice_path,
        invoice_request.clone(),
    )
    .await;
    assert_eq!(
        call(&app, &cookie, "POST", &invoice_path, invoice_request).await,
        invoice
    );
    assert_eq!(amount(&invoice, "total"), Decimal::from(150000));
    let invoice_id = id(&invoice);
    let payment = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/invoices/{invoice_id}/payments"),
        json!({
            "amount":"150000","idempotency_key":Uuid::new_v4()
        }),
    )
    .await;
    assert_eq!(payment["status"], "paid");
    let (status, dashboard, _) = request(&app, "GET", "/api/dashboard", Some(&cookie), None).await;
    assert!(status.is_success());
    assert_eq!(amount(&dashboard, "revenue"), Decimal::from(150000));
    assert_eq!(amount(&dashboard, "cash"), Decimal::from(150000));
    assert_eq!(amount(&dashboard, "inventory_value"), Decimal::ZERO);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM inventory_movements")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM shipment_postings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM journal_lines WHERE account_code IN ('1300','5000')"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments")
        .fetch_one(&pool)
        .await
        .unwrap();
    reverse(
        &app,
        &cookie,
        &format!("/api/invoices/{invoice_id}/payments/{payment_id}/reverse"),
    )
    .await;
    reverse(
        &app,
        &cookie,
        &format!("/api/invoices/{invoice_id}/reverse"),
    )
    .await;
    let (status, restored, _) = request(
        &app,
        "GET",
        &format!("/api/orders/{order_id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert!(status.is_success());
    assert_eq!(restored["status"], "confirmed");
    assert_eq!(restored["items"][0]["name"], "Consultation");
    assert_eq!(restored["items"][0]["product_kind"], "service");
    action(&app, &cookie, &invoice_path).await;
    let balance: Decimal = sqlx::query_scalar(
        "SELECT quantity+average_cost+inventory_value FROM products WHERE id=$1",
    )
    .bind(Uuid::parse_str(service_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(balance, Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations")]
async fn mixed_order_ships_only_stock_and_reverses_stock_only(pool: PgPool) {
    let (app, cookie, customer) = setup(&pool).await;
    let stock = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({"sku":"COFFEE","name":"Coffee","price":"80"}),
    )
    .await;
    assert_eq!(stock["kind"], "stock", "existing clients default to stock");
    let service = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({"sku":"DELIVERY","name":"Delivery","kind":"service","price":"100"}),
    )
    .await;
    let stock_id = id(&stock);
    let service_id = id(&service);
    call(&app, &cookie, "POST", "/api/inventory/receipts", json!({"product_id":stock_id,"quantity":"10","unit_cost":"50","idempotency_key":Uuid::new_v4()})).await;
    let (status, _, _) = request(
        &app,
        "PATCH",
        &format!("/api/products/{stock_id}"),
        Some(&cookie),
        Some(json!({
            "sku":"COFFEE","name":"Coffee","kind":"service","price":"80"
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "stock movements prevent kind changes before any order exists"
    );
    let order = call(
        &app,
        &cookie,
        "POST",
        "/api/orders",
        json!({"customer_id":customer,"items":[
        {"product_id":stock_id,"quantity":"2","unit_price":"80"},
        {"product_id":service_id,"quantity":"1.25","unit_price":"100"}
    ],"idempotency_key":Uuid::new_v4()}),
    )
    .await;
    assert_eq!(order["has_stock_items"], true);
    let order_id = id(&order);
    action(&app, &cookie, &format!("/api/orders/{order_id}/confirm")).await;
    let (status, _, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order_id}/invoice"),
        Some(&cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let shipment = json!({"idempotency_key":Uuid::new_v4()});
    let path = format!("/api/orders/{order_id}/ship");
    let shipped = call(&app, &cookie, "POST", &path, shipment.clone()).await;
    assert_eq!(call(&app, &cookie, "POST", &path, shipment).await, shipped);
    let invoice = action(&app, &cookie, &format!("/api/orders/{order_id}/invoice")).await;
    assert_eq!(amount(&invoice, "total"), Decimal::from(285));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_movements WHERE kind='shipment'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    let stock_quantity: Decimal = sqlx::query_scalar("SELECT quantity FROM products WHERE id=$1")
        .bind(Uuid::parse_str(stock_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stock_quantity, Decimal::from(8));
    let cogs: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='5000'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cogs, Decimal::from(100));
    reverse(
        &app,
        &cookie,
        &format!("/api/invoices/{}/reverse", id(&invoice)),
    )
    .await;
    let (status, after, _) = request(
        &app,
        "GET",
        &format!("/api/orders/{order_id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert!(status.is_success());
    assert_eq!(after["status"], "shipped");
    reverse(
        &app,
        &cookie,
        &format!("/api/orders/{order_id}/reverse-shipment"),
    )
    .await;
    let restored_quantity: Decimal =
        sqlx::query_scalar("SELECT quantity FROM products WHERE id=$1")
            .bind(Uuid::parse_str(stock_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(restored_quantity, Decimal::from(10));
    let service_movements: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inventory_movements WHERE product_id=$1")
            .bind(Uuid::parse_str(service_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(service_movements, 0);
    let mutation = sqlx::query(
        "UPDATE sales_order_lines SET product_kind='stock' WHERE order_id=$1 AND product_id=$2",
    )
    .bind(Uuid::parse_str(order_id).unwrap())
    .bind(Uuid::parse_str(service_id).unwrap())
    .execute(&pool)
    .await;
    assert!(
        mutation.is_err(),
        "fulfilment kind snapshots must be immutable"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn zero_cost_mixed_shipment_reverses_and_unused_kind_changes_are_validated(pool: PgPool) {
    let (app, cookie, customer) = setup(&pool).await;
    let unused = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({"sku":"UNUSED","name":"Unused","kind":"service","price":"100"}),
    )
    .await;
    let changed = call(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/products/{}", id(&unused)),
        json!({"sku":"UNUSED","name":"Unused","kind":"stock","price":"100"}),
    )
    .await;
    assert_eq!(changed["kind"], "stock");
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/products",
        Some(&cookie),
        Some(json!({"sku":"BAD","name":"Bad kind","kind":"virtual","price":"1"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let stock = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({"sku":"FRACTION","name":"Fractional stock","price":"100"}),
    )
    .await;
    let service = call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        json!({"sku":"FEE","name":"Service fee","kind":"service","price":"100"}),
    )
    .await;
    call(&app, &cookie, "POST", "/api/inventory/receipts", json!({"product_id":id(&stock),"quantity":"1000","unit_cost":"0.0001","idempotency_key":Uuid::new_v4()})).await;
    let order = call(
        &app,
        &cookie,
        "POST",
        "/api/orders",
        json!({"customer_id":customer,"items":[
        {"product_id":id(&stock),"quantity":"0.001","unit_price":"100"},
        {"product_id":id(&service),"quantity":"1","unit_price":"100"}
    ],"idempotency_key":Uuid::new_v4()}),
    )
    .await;
    let order_id = id(&order);
    action(&app, &cookie, &format!("/api/orders/{order_id}/confirm")).await;
    action(&app, &cookie, &format!("/api/orders/{order_id}/ship")).await;
    let value: Decimal =
        sqlx::query_scalar("SELECT value FROM inventory_movements WHERE kind='shipment'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(value, Decimal::ZERO);
    reverse(
        &app,
        &cookie,
        &format!("/api/orders/{order_id}/reverse-shipment"),
    )
    .await;
    let quantity: Decimal = sqlx::query_scalar("SELECT quantity FROM products WHERE id=$1")
        .bind(Uuid::parse_str(id(&stock)).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(quantity, Decimal::from(1000));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM journal_lines WHERE account_code='5000'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    for column in ["quantity", "average_cost", "inventory_value"] {
        let invalid = sqlx::query(&format!("UPDATE products SET {column}=1 WHERE id=$1"))
            .bind(Uuid::parse_str(id(&service)).unwrap())
            .execute(&pool)
            .await;
        assert!(invalid.is_err(), "service {column} must remain zero");
    }
}
