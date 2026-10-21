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
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Customer return tester',$2,'unused',$3)").bind(user).bind(format!("{user}@example.test")).bind(role).execute(pool).await.unwrap();
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
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Customer returns')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let cookie = actor(pool, "admin").await;
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "sales-return-fixture-token-at-least-32-characters".into(),
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
        Some(json!({"sku":"RET-STOCK","name":"Stock","price":"5"})),
    )
    .await;
    let service=call(&app,&cookie,"/api/products",Some(json!({"sku":"RET-SERVICE","name":"Service","kind":"service","unit":"hour","price":"5"}))).await;
    call(&app,&cookie,"/api/inventory/receipts",Some(json!({"product_id":stock["id"],"quantity":"3","unit_cost":"3.3333","idempotency_key":Uuid::new_v4()}))).await;
    (app, cookie, uid(&customer), uid(&stock), uid(&service))
}
fn uid(value: &Value) -> Uuid {
    value["id"].as_str().unwrap().parse().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn amount(value: &Value, field: &str) -> Decimal {
    d(value[field].as_str().unwrap())
}
async fn invoice(app: &Router, cookie: &str, customer: Uuid, items: Value, ship: bool) -> Value {
    let order = call(
        app,
        cookie,
        "/api/orders",
        Some(json!({"customer_id":customer,"items":items,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let id = uid(&order);
    call(
        app,
        cookie,
        &format!("/api/orders/{id}/confirm"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    if ship {
        call(
            app,
            cookie,
            &format!("/api/orders/{id}/ship"),
            Some(json!({"idempotency_key":Uuid::new_v4()})),
        )
        .await;
    }
    call(
        app,
        cookie,
        &format!("/api/orders/{id}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
fn return_body(invoice: &Value, sku: &str, quantity: &str, confirmed: bool) -> Value {
    let item = invoice["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["sku"] == sku)
        .unwrap();
    json!({"version":invoice["version"],"reason":"Customer returned incorrect goods","refund_confirmed":confirmed,"items":[{"invoice_line_id":item["id"],"quantity":quantity}],"idempotency_key":Uuid::new_v4()})
}
fn reverse_body(invoice: &Value) -> Value {
    json!({"version":invoice["version"],"reason":"Customer return correction","idempotency_key":Uuid::new_v4()})
}
async fn stock_balance(pool: &PgPool, product: Uuid) -> (Decimal, Decimal, Decimal) {
    let row = sqlx::query("SELECT quantity,inventory_value,average_cost FROM products WHERE id=$1")
        .bind(product)
        .fetch_one(pool)
        .await
        .unwrap();
    (
        row.get("quantity"),
        row.get("inventory_value"),
        row.get("average_cost"),
    )
}
async fn cash(pool: &PgPool) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_return_retained_credit_180_120_80_can_allocate_15_without_extra_cash(
    pool: PgPool,
) {
    let (app, cookie, customer, stock, service) = fixture(&pool).await;
    let original=invoice(&app,&cookie,customer,json!([{"product_id":stock,"quantity":"1","unit_price":"100"},{"product_id":service,"quantity":"1","unit_price":"80"}]),true).await;
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/payments", uid(&original)),
        Some(json!({"amount":"120","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let before = cash(&pool).await;
    let mut input = return_body(&paid, "RET-SERVICE", "1", false);
    input["refund_mode"] = json!("credit");
    let returned = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/returns", uid(&paid)),
        Some(input.clone()),
    )
    .await;
    assert_eq!(
        call(
            &app,
            &cookie,
            &format!("/api/invoices/{}/returns", uid(&paid)),
            Some(input)
        )
        .await,
        returned
    );
    assert_eq!(amount(&returned, "net_total"), d("100"));
    assert_eq!(amount(&returned, "paid"), d("100"));
    assert_eq!(cash(&pool).await, before);
    let source = &returned["returns"][0];
    assert_eq!(amount(source, "refund"), Decimal::ZERO);
    assert_eq!(amount(source, "settlement_credit"), d("20"));
    assert_eq!(source["refund_mode"], "credit");
    let fund = call(
        &app,
        &cookie,
        &format!(
            "/api/party-funds/{}",
            source["credit_fund_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(amount(&fund, "available"), d("20"));
    let next = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"1","unit_price":"15"}]),
        false,
    )
    .await;
    let used=call(&app,&cookie,&format!("/api/party-funds/{}/allocate",uid(&fund)),Some(json!({"version":fund["version"],"items":[{"target_type":"invoice","target_id":next["id"],"amount":"15","version":next["version"]}],"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&used, "available"), d("5"));
    assert_eq!(cash(&pool).await, before);
    let refunded=call(&app,&cookie,&format!("/api/party-funds/{}/refunds",uid(&used)),Some(json!({"version":used["version"],"amount":"5","reason":"Sisa kredit benar-benar dikembalikan pelanggan","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&refunded, "available"), Decimal::ZERO);
    assert_eq!(amount(&refunded, "refunded"), d("5"));
    assert_eq!(cash(&pool).await, before - d("5"));
    assert_eq!(
        request(
            &app,
            &cookie,
            &format!(
                "/api/sales-returns/{}/reverse",
                source["id"].as_str().unwrap()
            ),
            Some(reverse_body(&returned))
        )
        .await
        .0,
        409
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_return_deferred_refund_reserves_credit_and_cash_moves_only_when_paid(
    pool: PgPool,
) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let original = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"2","unit_price":"5"}]),
        false,
    )
    .await;
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/payments", uid(&original)),
        Some(json!({"amount":"10","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let mut input = return_body(&paid, "RET-SERVICE", "1", false);
    input["refund_mode"] = json!("deferred");
    let returned = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/returns", uid(&paid)),
        Some(input),
    )
    .await;
    let source = &returned["returns"][0];
    assert_eq!(amount(source, "refund"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, d("10"));
    assert_eq!(source["can_reverse"], false);
    let fund = call(
        &app,
        &cookie,
        &format!(
            "/api/party-funds/{}",
            source["credit_fund_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(amount(&fund, "reserved"), d("5"));
    assert_eq!(amount(&fund, "available"), Decimal::ZERO);
    let request_id = source["refund_request_id"].as_str().unwrap();
    let refunded=call(&app,&cookie,&format!("/api/party-refund-requests/{request_id}/pay"),Some(json!({"version":fund["version"],"amount":"2","reason":"Pengembalian dana sebagian telah diterima pelanggan","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(cash(&pool).await, d("8"));
    assert_eq!(amount(&refunded, "reserved"), d("3"));
    let cancelled=call(&app,&cookie,&format!("/api/party-refund-requests/{request_id}/cancel"),Some(json!({"version":refunded["version"],"reason":"Sisa permintaan dibatalkan untuk koreksi","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&cancelled, "available"), d("3"));
    let restored=call(&app,&cookie,&format!("/api/party-refunds/{}/reverse",refunded["refunds"][0]["id"].as_str().unwrap()),Some(json!({"version":cancelled["version"],"reason":"Dana sebagian dikembalikan untuk koreksi","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&restored, "available"), d("5"));
    assert_eq!(cash(&pool).await, d("10"));
    let reversed = call(
        &app,
        &cookie,
        &format!(
            "/api/sales-returns/{}/reverse",
            source["id"].as_str().unwrap()
        ),
        Some(reverse_body(&returned)),
    )
    .await;
    assert_eq!(amount(&reversed, "paid"), d("10"));
    assert_eq!(amount(&reversed, "credit_total"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, d("10"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unpaid_return_reduces_receivable_and_restores_exact_stock_on_reversal(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"2","unit_price":"5"}]),
        true,
    )
    .await;
    let before = stock_balance(&pool, stock).await;
    let path = format!("/api/invoices/{}/returns", uid(&invoice));
    let body = return_body(&invoice, "RET-STOCK", "1", false);
    let returned = call(&app, &cookie, &path, Some(body.clone())).await;
    assert_eq!(returned, call(&app, &cookie, &path, Some(body)).await);
    assert_eq!(amount(&returned, "credit_total"), d("5"));
    assert_eq!(amount(&returned, "outstanding"), d("5"));
    assert_eq!(returned["status"], "unpaid");
    let original = &returned["returns"][0];
    assert_eq!(amount(original, "cost_total"), d("3.34"));
    assert_eq!(amount(original, "refund"), Decimal::ZERO);
    assert_eq!(stock_balance(&pool, stock).await.0, d("2"));
    assert_eq!(stock_balance(&pool, stock).await.1, d("6.67"));
    let detail = call(
        &app,
        &cookie,
        &format!("/api/sales-returns/{}", uid(original)),
        None,
    )
    .await;
    assert_eq!(&detail, original);
    let reversed = call(
        &app,
        &cookie,
        &format!("/api/sales-returns/{}/reverse", uid(original)),
        Some(reverse_body(&returned)),
    )
    .await;
    assert_eq!(amount(&reversed, "credit_total"), Decimal::ZERO);
    assert_eq!(amount(&reversed, "outstanding"), d("10"));
    assert_eq!(stock_balance(&pool, stock).await, before);
    assert!(reversed["returns"][0]["reversal"].is_object());
    let order = call(
        &app,
        &cookie,
        &format!("/api/orders/{}", invoice["order_id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(order["status"], "invoiced");
    assert_eq!(order["can_ship"], false);
    assert_eq!(order["can_invoice"], false);
    let balance: Decimal = sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations")]
async fn paid_return_requires_explicit_refund_and_blocks_dependent_reversals(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"2","unit_price":"5"}]),
        true,
    )
    .await;
    let invoice_id = uid(&invoice);
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{invoice_id}/payments"),
        Some(json!({"amount":"10","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let path = format!("/api/invoices/{invoice_id}/returns");
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&paid, "RET-STOCK", "1", false)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let returned = call(
        &app,
        &cookie,
        &path,
        Some(return_body(&paid, "RET-STOCK", "1", true)),
    )
    .await;
    assert_eq!(amount(&returned, "net_total"), d("5"));
    assert_eq!(amount(&returned, "paid"), d("5"));
    assert_eq!(returned["status"], "paid");
    assert_eq!(cash(&pool).await, d("5"));
    assert_eq!(amount(&returned["returns"][0], "refund"), d("5"));
    let payment: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE invoice_id=$1")
        .bind(invoice_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/invoices/{invoice_id}/payments/{payment}/reverse"),
        Some(json!({"reason":"Cannot reverse dependent payment","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/invoices/{invoice_id}/reverse"),
        Some(json!({"reason":"Cannot reverse dependent invoice","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let reversed = call(
        &app,
        &cookie,
        &format!(
            "/api/sales-returns/{}/reverse",
            uid(&returned["returns"][0])
        ),
        Some(reverse_body(&returned)),
    )
    .await;
    assert_eq!(amount(&reversed, "paid"), d("10"));
    assert_eq!(cash(&pool).await, d("10"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn partial_payment_and_full_service_return_conserve_cash_and_invoice_balance(pool: PgPool) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"2","unit_price":"5"}]),
        false,
    )
    .await;
    let id = uid(&invoice);
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/payments"),
        Some(json!({"amount":"7","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let first = call(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/returns"),
        Some(return_body(&paid, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(amount(&first, "paid"), d("5"));
    assert_eq!(amount(&first["returns"][0], "refund"), d("2"));
    assert_eq!(cash(&pool).await, d("5"));
    let final_return = call(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/returns"),
        Some(return_body(&first, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(amount(&final_return, "net_total"), Decimal::ZERO);
    assert_eq!(amount(&final_return, "paid"), Decimal::ZERO);
    assert_eq!(final_return["status"], "paid");
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    let movements: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inventory_movements WHERE kind='sales_return'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(movements, 0);
    let insights = call(&app, &cookie, "/api/dashboard/insights", None).await;
    assert_eq!(amount(&insights["receivables"], "invoiced"), Decimal::ZERO);
    assert_eq!(
        amount(&insights["receivables"], "outstanding"),
        Decimal::ZERO
    );
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/payments"),
        Some(json!({"amount":"0.01","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_returns_use_historical_delivery_cost_after_new_receipts(pool: PgPool) {
    let (app, cookie, customer, stock, service) = fixture(&pool).await;
    let invoice=invoice(&app,&cookie,customer,json!([{"product_id":stock,"quantity":"3","unit_price":"5"},{"product_id":service,"quantity":"1","unit_price":"10"}]),true).await;
    call(&app,&cookie,"/api/inventory/receipts",Some(json!({"product_id":stock,"quantity":"1","unit_cost":"20","idempotency_key":Uuid::new_v4()}))).await;
    let first = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/returns", uid(&invoice)),
        Some(return_body(&invoice, "RET-STOCK", "1", false)),
    )
    .await;
    assert_eq!(amount(&first["returns"][0], "cost_total"), d("3.33"));
    let second = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/returns", uid(&invoice)),
        Some(return_body(&first, "RET-STOCK", "2", false)),
    )
    .await;
    assert_eq!(amount(&second["returns"][1], "cost_total"), d("6.67"));
    let balance = stock_balance(&pool, stock).await;
    assert_eq!(balance.0, d("4"));
    assert_eq!(balance.1, d("30"));
    assert_eq!(balance.2, d("7.5"));
    let old_return = uid(&second["returns"][0]);
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/sales-returns/{old_return}/reverse"),
        Some(reverse_body(&second)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let last_return = uid(&second["returns"][1]);
    let restored = call(
        &app,
        &cookie,
        &format!("/api/sales-returns/{last_return}/reverse"),
        Some(reverse_body(&second)),
    )
    .await;
    assert_eq!(amount(&restored, "credit_total"), d("5"));
    assert!(
        sqlx::query("UPDATE sales_return_lines SET cost=0")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM sales_returns")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_customer_returns_and_overpayments_cannot_exceed_net_balance(pool: PgPool) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"2","unit_price":"5"}]),
        false,
    )
    .await;
    let id = uid(&invoice);
    let path = format!("/api/invoices/{id}/returns");
    let (a, b) = tokio::join!(
        request(
            &app,
            &cookie,
            &path,
            Some(return_body(&invoice, "RET-SERVICE", "1.5", false))
        ),
        request(
            &app,
            &cookie,
            &path,
            Some(return_body(&invoice, "RET-SERVICE", "1.5", false))
        )
    );
    assert_eq!([a.0, b.0].iter().filter(|s| s.is_success()).count(), 1);
    assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
    let current = call(&app, &cookie, &format!("/api/invoices/{id}"), None).await;
    assert_eq!(amount(&current, "outstanding"), d("2.5"));
    let (status, _) = request(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/payments"),
        Some(json!({"amount":"3","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/payments"),
        Some(json!({"amount":"2.5","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(paid["status"], "paid");
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&paid, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sales_returns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_return_roles_closed_periods_validation_and_cash_caps_are_atomic(pool: PgPool) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"1","unit_price":"5"}]),
        false,
    )
    .await;
    let id = uid(&invoice);
    let path = format!("/api/invoices/{id}/returns");
    for role in ["warehouse", "sales", "viewer"] {
        let other = actor(&pool, role).await;
        let (status, _) = request(
            &app,
            &other,
            &path,
            Some(return_body(&invoice, "RET-SERVICE", "1", false)),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let mut invalid = return_body(&invoice, "RET-SERVICE", "1", false);
    invalid["reason"] = json!("bad\u{0000}reason");
    let (status, _) = request(&app, &cookie, &path, Some(invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut invalid = return_body(&invoice, "RET-SERVICE", "1", false);
    invalid["items"][0]["invoice_line_id"] = json!(Uuid::new_v4());
    let (status, _) = request(&app, &cookie, &path, Some(invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let paid = call(
        &app,
        &cookie,
        &format!("/api/invoices/{id}/payments"),
        Some(json!({"amount":"5","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    call(&app,&cookie,"/api/accounting/journals",Some(json!({"description":"Cash spent before customer refund","lines":[{"account_code":"5000","debit":"5","credit":"0"},{"account_code":"1100","debit":"0","credit":"5"}],"idempotency_key":Uuid::new_v4()}))).await;
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&paid, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sales_returns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&paid, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_return_rejects_unit_changes_and_later_stock_reversal(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":stock,"quantity":"1","unit_price":"5"}]),
        true,
    )
    .await;
    let path = format!("/api/invoices/{}/returns", uid(&invoice));
    sqlx::query("UPDATE products SET unit='box' WHERE id=$1")
        .bind(stock)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&invoice, "RET-STOCK", "1", false)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    sqlx::query("UPDATE products SET unit='pcs' WHERE id=$1")
        .bind(stock)
        .execute(&pool)
        .await
        .unwrap();
    let returned = call(
        &app,
        &cookie,
        &path,
        Some(return_body(&invoice, "RET-STOCK", "1", false)),
    )
    .await;
    call(&app, &cookie, "/api/inventory/receipts", Some(json!({"product_id":stock,"quantity":"1","unit_cost":"4","idempotency_key":Uuid::new_v4()}))).await;
    let (status, _) = request(
        &app,
        &cookie,
        &format!(
            "/api/sales-returns/{}/reverse",
            uid(&returned["returns"][0])
        ),
        Some(reverse_body(&returned)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tiny_customer_credit_preserves_the_final_quantity(pool: PgPool) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"1","unit_price":"0.01"}]),
        false,
    )
    .await;
    let path = format!("/api/invoices/{}/returns", uid(&invoice));
    let (status, _) = request(
        &app,
        &cookie,
        &path,
        Some(return_body(&invoice, "RET-SERVICE", "0.999", false)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let returned = call(
        &app,
        &cookie,
        &path,
        Some(return_body(&invoice, "RET-SERVICE", "1", false)),
    )
    .await;
    assert_eq!(returned["status"], "paid");
    assert_eq!(amount(&returned, "net_total"), Decimal::ZERO);
    assert_eq!(amount(&returned, "credit_total"), d("0.01"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn returns_across_partial_deliveries_cannot_reclaim_more_historical_cost(pool: PgPool) {
    let (app, cookie, customer, stock, _) = fixture(&pool).await;
    let order = call(&app, &cookie, "/api/orders", Some(json!({"customer_id":customer,"items":[{"product_id":stock,"quantity":"3","unit_price":"5"}],"idempotency_key":Uuid::new_v4()}))).await;
    let order_id = uid(&order);
    call(
        &app,
        &cookie,
        &format!("/api/orders/{order_id}/confirm"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    for quantity in ["1", "2"] {
        call(&app, &cookie, &format!("/api/orders/{order_id}/ship"), Some(json!({"items":[{"line_id":order["items"][0]["id"],"quantity":quantity}],"idempotency_key":Uuid::new_v4()}))).await;
    }
    let invoice = call(
        &app,
        &cookie,
        &format!("/api/orders/{order_id}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let path = format!("/api/invoices/{}/returns", uid(&invoice));
    let first = call(
        &app,
        &cookie,
        &path,
        Some(return_body(&invoice, "RET-STOCK", "1.5", false)),
    )
    .await;
    let final_return = call(
        &app,
        &cookie,
        &path,
        Some(return_body(&first, "RET-STOCK", "1.5", false)),
    )
    .await;
    assert_eq!(amount(&final_return, "credit_total"), d("15"));
    let total: Decimal = sqlx::query_scalar("SELECT sum(cost) FROM sales_return_sources")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, d("10"));
    let balance = stock_balance(&pool, stock).await;
    assert_eq!(balance.0, d("3"));
    assert_eq!(balance.1, d("10"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_payment_accepts_large_invoice_money_and_rejects_control_text_atomically(
    pool: PgPool,
) {
    let (app, cookie, customer, _, service) = fixture(&pool).await;
    let invoice = invoice(
        &app,
        &cookie,
        customer,
        json!([{"product_id":service,"quantity":"2","unit_price":"1000000000"}]),
        false,
    )
    .await;
    let path = format!("/api/invoices/{}/payments", uid(&invoice));
    let (status, _) = request(&app, &cookie, &path, Some(json!({"amount":"1500000000","reference":"bad\u{0000}reference","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let paid = call(&app, &cookie, &path, Some(json!({"amount":"1500000000","reference":"Large customer payment","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&paid, "paid"), d("1500000000"));
    let returned = call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/returns", uid(&invoice)),
        Some(return_body(&paid, "RET-SERVICE", "1", true)),
    )
    .await;
    assert_eq!(amount(&returned["returns"][0], "refund"), d("500000000"));
    assert_eq!(amount(&returned, "net_total"), d("1000000000"));
    assert_eq!(amount(&returned, "paid"), d("1000000000"));
}
