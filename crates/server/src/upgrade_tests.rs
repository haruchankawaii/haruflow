use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::borrow::Cow;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(app: &Router, cookie: &str, path: &str, body: Option<Value>) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(if body.is_some() { "POST" } else { "GET" })
                .uri(path)
                .header("cookie", cookie)
                .header("x-haruflow-request", "1")
                .header("content-type", "application/json")
                .body(
                    body.map(|value| Body::from(value.to_string()))
                        .unwrap_or_default(),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(status.is_success(), "{path}: {status} {value}");
    value
}

#[sqlx::test(migrations = false)]
async fn upgrade_preserves_existing_stock_orders_and_ledgers(pool: PgPool) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|migration| migration.version <= 5)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    let company = Uuid::new_v4();
    let user = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let product = Uuid::new_v4();
    let order = Uuid::new_v4();
    let movement = Uuid::new_v4();
    let journal = Uuid::new_v4();
    let token = "b".repeat(64);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Existing company')")
        .bind(company)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Existing admin','upgrade@example.test','fixture-not-used-for-login','admin')").bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '1 day')",
    )
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .bind(user)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO customers(id,name) VALUES($1,'Existing customer')")
        .bind(customer)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO products(id,sku,name,price,quantity,average_cost,inventory_value) VALUES($1,'LEGACY','Existing product',80,4,50,200)").bind(product).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO sales_orders(id,number,customer_id,total,created_by) VALUES($1,'SO-'||lpad(nextval('sales_order_number')::text,6,'0'),$2,80,$3)").bind(order).bind(customer).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO sales_order_lines(id,order_id,product_id,product_sku,product_name,quantity,unit_price,total) VALUES($1,$2,$3,'LEGACY','Existing product',1,80,80)").bind(Uuid::new_v4()).bind(order).bind(product).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,created_by) VALUES($1,$2,'receipt',4,50,200,$3)").bind(movement).bind(product).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Existing opening stock','receipt',$2,$3)").bind(journal).bind(movement).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1300',200,0),($3,$2,'3000',0,200)").bind(Uuid::new_v4()).bind(journal).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    // Re-running an upgrade must be safe and must not duplicate opening data.
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "upgrade-test-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        "apps/web/dist".into(),
    );
    let cookie = format!("hf_session={token}");
    let opening = call(&app, &cookie, "/api/accounting/opening", None).await;
    assert_eq!(opening["eligible"], false);
    assert!(opening["document"].is_null());
    let products = call(&app, &cookie, "/api/products", None).await;
    assert_eq!(products["items"][0]["kind"], "stock");
    assert_eq!(products["items"][0]["quantity"], "4.000");
    let detail = call(&app, &cookie, &format!("/api/orders/{order}"), None).await;
    assert_eq!(detail["items"][0]["product_kind"], "stock");
    assert_eq!(detail["number"], "SO-000001");
    for action in ["confirm", "ship"] {
        call(
            &app,
            &cookie,
            &format!("/api/orders/{order}/{action}"),
            Some(json!({"idempotency_key":Uuid::new_v4()})),
        )
        .await;
    }
    let invoice = call(
        &app,
        &cookie,
        &format!("/api/orders/{order}/invoice"),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(invoice["number"], "INV-000001");
    call(
        &app,
        &cookie,
        &format!("/api/invoices/{}/payments", invoice["id"].as_str().unwrap()),
        Some(json!({"amount":"80","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let balance = call(&app, &cookie, "/api/accounting/trial-balance", None).await;
    assert_eq!(balance["total_debit"], "280.00");
    assert_eq!(balance["total_credit"], "280.00");
    let preserved:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=$1 AND description='Existing opening stock')").bind(journal).fetch_one(&pool).await.unwrap();
    assert!(preserved);
}
