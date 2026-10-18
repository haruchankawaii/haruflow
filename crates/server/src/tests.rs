use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{AppState, app_router};

const TOKEN: &str = "integration-setup-token-at-least-32-characters";

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
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, value, cookie)
}

#[track_caller]
fn success(status: StatusCode, value: &Value) {
    assert!(status.is_success(), "{status}: {value}");
}

async fn create_order(
    app: &Router,
    cookie: &str,
    customer: &str,
    product: &str,
    quantity: &str,
) -> String {
    let (status, order, _) = request(app, "POST", "/api/orders", Some(cookie), Some(json!({"customer_id":customer,"items":[{"product_id":product,"quantity":quantity,"unit_price":"80000"}],"idempotency_key":Uuid::new_v4()}))).await;
    success(status, &order);
    let id = order["id"].as_str().unwrap().to_owned();
    let (status, value, _) = request(
        app,
        "POST",
        &format!("/api/orders/{id}/confirm"),
        Some(cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &value);
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn complete_erp_workflow_and_invariants(pool: PgPool) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .try_init();
    let app = app_router(
        AppState {
            db: pool.clone(),
            setup_token: TOKEN.into(),
            secure_cookies: false,
        },
        "apps/web/dist".into(),
    );
    let (status, _, _) = request(&app, "GET", "/api/products", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let unsafe_request = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(unsafe_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (status, _, _) = request(&app, "GET", "/health/ready", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let setup = json!({"token":TOKEN,"company_name":"Toko Uji Haruflow","name":"Administrator","email":"admin@example.test","password":"test-password-very-strong"});
    let (status, value, cookie) =
        request(&app, "POST", "/api/setup", None, Some(setup.clone())).await;
    success(status, &value);
    let cookie = cookie.unwrap();
    let (status, _, _) = request(&app, "POST", "/api/setup", None, Some(setup)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, customer, _) = request(
        &app,
        "POST",
        "/api/customers",
        Some(&cookie),
        Some(json!({"name":"Pelanggan Uji"})),
    )
    .await;
    success(status, &customer);
    let customer = customer["id"].as_str().unwrap();
    let (status, product, _) = request(
        &app,
        "POST",
        "/api/products",
        Some(&cookie),
        Some(json!({"sku":"UJI-001","name":"Produk Uji","price":"80000"})),
    )
    .await;
    success(status, &product);
    let product = product["id"].as_str().unwrap();
    let receipt = json!({"product_id":product,"quantity":"10","unit_cost":"50000","idempotency_key":Uuid::new_v4()});
    let (status, receipt_result, _) = request(
        &app,
        "POST",
        "/api/inventory/receipts",
        Some(&cookie),
        Some(receipt.clone()),
    )
    .await;
    success(status, &receipt_result);
    let (status, repeated, _) = request(
        &app,
        "POST",
        "/api/inventory/receipts",
        Some(&cookie),
        Some(receipt.clone()),
    )
    .await;
    success(status, &repeated);
    assert_eq!(repeated, receipt_result);
    let mut changed = receipt;
    changed["quantity"] = json!("11");
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/inventory/receipts",
        Some(&cookie),
        Some(changed),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let order = create_order(&app, &cookie, customer, product, "2").await;
    let shipment = json!({"idempotency_key":Uuid::new_v4()});
    for _ in 0..2 {
        let (status, value, _) = request(
            &app,
            "POST",
            &format!("/api/orders/{order}/ship"),
            Some(&cookie),
            Some(shipment.clone()),
        )
        .await;
        success(status, &value);
    }
    let (status, invoice, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order}/invoice"),
        Some(&cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &invoice);
    let invoice = invoice["id"].as_str().unwrap();
    let first_payment =
        json!({"amount":"60000","reference":"Transfer uji","idempotency_key":Uuid::new_v4()});
    for _ in 0..2 {
        let (status, value, _) = request(
            &app,
            "POST",
            &format!("/api/invoices/{invoice}/payments"),
            Some(&cookie),
            Some(first_payment.clone()),
        )
        .await;
        success(status, &value);
    }
    let (status, _, _) = request(
        &app,
        "POST",
        &format!("/api/invoices/{invoice}/payments"),
        Some(&cookie),
        Some(json!({"amount":"100001","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, value, _) = request(
        &app,
        "POST",
        &format!("/api/invoices/{invoice}/payments"),
        Some(&cookie),
        Some(json!({"amount":"100000","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &value);
    let (status, dashboard, _) = request(&app, "GET", "/api/dashboard", Some(&cookie), None).await;
    success(status, &dashboard);
    for (key, expected) in [
        ("inventory_value", 400000),
        ("cash", 160000),
        ("revenue", 160000),
        ("outstanding", 0),
    ] {
        assert_eq!(
            dashboard[key]
                .as_str()
                .unwrap()
                .parse::<rust_decimal::Decimal>()
                .unwrap(),
            rust_decimal::Decimal::from(expected),
            "{key}"
        );
    }
    let journal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(journal_count, 5, "receipt, shipment, invoice, two payments");
    let mismatch: bool = sqlx::query_scalar("SELECT EXISTS(SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING SUM(debit) <> SUM(credit))").fetch_one(&pool).await.unwrap();
    assert!(!mismatch);
    let (status, viewer, _) = request(&app, "POST", "/api/users", Some(&cookie), Some(json!({"name":"Pembaca","email":"viewer@example.test","password":"viewer-password-strong","role":"viewer"}))).await;
    success(status, &viewer);
    let (status, value, viewer_cookie) = request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email":"viewer@example.test","password":"viewer-password-strong"})),
    )
    .await;
    success(status, &value);
    let viewer_cookie = viewer_cookie.unwrap();
    let (status, _, _) = request(&app, "GET", "/api/products", Some(&viewer_cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/products",
        Some(&viewer_cookie),
        Some(json!({"sku":"NO","name":"Forbidden","price":"1"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Two orders compete for the last eight units: only one shipment may commit.
    let a = create_order(&app, &cookie, customer, product, "7").await;
    let b = create_order(&app, &cookie, customer, product, "7").await;
    let a_path = format!("/api/orders/{a}/ship");
    let b_path = format!("/api/orders/{b}/ship");
    let (a, b) = tokio::join!(
        request(
            &app,
            "POST",
            &a_path,
            Some(&cookie),
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        ),
        request(
            &app,
            "POST",
            &b_path,
            Some(&cookie),
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        )
    );
    assert_eq!([a.0, b.0].iter().filter(|s| s.is_success()).count(), 1);
    assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
    let quantity: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT quantity FROM products WHERE id=$1")
            .bind(Uuid::parse_str(product).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(quantity, rust_decimal::Decimal::ONE);
    let entry = sqlx::query("SELECT id,created_by FROM journal_entries LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let entry_id: Uuid = entry.get("id");
    assert!(
        sqlx::query("UPDATE journal_lines SET debit=debit+1 WHERE journal_id=$1 AND debit>0")
            .bind(entry_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1100',1,0)").bind(Uuid::new_v4()).bind(entry_id).execute(&pool).await.is_err());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'invalid','receipt',$2,$3)")
        .bind(Uuid::new_v4()).bind(Uuid::new_v4()).bind(entry.get::<Uuid,_>("created_by")).execute(&mut *tx).await.unwrap();
    assert!(
        tx.commit().await.is_err(),
        "unbalanced/empty journal must not commit"
    );
    // Corrections preserve every historical posting and reverse in dependency order.
    let correction = || json!({"reason":"Koreksi transaksi uji","idempotency_key":Uuid::new_v4()});
    let (status, _, _) = request(
        &app,
        "POST",
        &format!("/api/invoices/{invoice}/reverse"),
        Some(&cookie),
        Some(correction()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "paid invoice cannot be reversed first"
    );
    let (status, history, _) = request(
        &app,
        "GET",
        &format!("/api/invoices/{invoice}/payments"),
        Some(&cookie),
        None,
    )
    .await;
    success(status, &history);
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    for payment in history["items"].as_array().unwrap() {
        let payment_id = payment["id"].as_str().unwrap();
        let body = correction();
        for _ in 0..2 {
            let (status, value, _) = request(
                &app,
                "POST",
                &format!("/api/invoices/{invoice}/payments/{payment_id}/reverse"),
                Some(&cookie),
                Some(body.clone()),
            )
            .await;
            success(status, &value);
        }
    }
    let (status, value, _) = request(
        &app,
        "POST",
        &format!("/api/invoices/{invoice}/reverse"),
        Some(&cookie),
        Some(correction()),
    )
    .await;
    success(status, &value);
    assert_eq!(value["status"], "void");
    let (status, value, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order}/reverse-shipment"),
        Some(&cookie),
        Some(correction()),
    )
    .await;
    success(status, &value);
    assert_eq!(value["status"], "confirmed");
    // Re-shipping uses a new posting ID; a void invoice does not block a replacement.
    let (status, value, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order}/ship"),
        Some(&cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &value);
    let (status, replacement, _) = request(
        &app,
        "POST",
        &format!("/api/orders/{order}/invoice"),
        Some(&cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &replacement);
    assert_ne!(replacement["id"].as_str().unwrap(), invoice);
    let manual = json!({"description":"Modal kas uji","lines":[{"account_code":"1100","debit":"500","credit":"0"},{"account_code":"3000","debit":"0","credit":"500"}],"idempotency_key":Uuid::new_v4()});
    let (status, value, _) = request(
        &app,
        "POST",
        "/api/accounting/journals",
        Some(&cookie),
        Some(manual.clone()),
    )
    .await;
    success(status, &value);
    let mut unbalanced = manual.clone();
    unbalanced["idempotency_key"] = json!(Uuid::new_v4());
    unbalanced["lines"][1]["credit"] = json!("499");
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/accounting/journals",
        Some(&cookie),
        Some(unbalanced),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let adjustment = json!({"product_id":product,"quantity":"-0.5","reason":"Selisih opname uji","idempotency_key":Uuid::new_v4()});
    let (status, value, _) = request(
        &app,
        "POST",
        "/api/inventory/adjustments",
        Some(&cookie),
        Some(adjustment),
    )
    .await;
    success(status, &value);
    let (status,_,_)=request(&app,"POST","/api/inventory/adjustments",Some(&cookie),Some(json!({"product_id":product,"quantity":"-10","reason":"Tidak boleh negatif","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let today: chrono::NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status,value,_)=request(&app,"POST","/api/accounting/period/close",Some(&cookie),Some(json!({"through":today.pred_opt().unwrap().to_string(),"reason":"Tutup hari sebelumnya","idempotency_key":Uuid::new_v4()}))).await;
    success(status, &value);
    let mut next_manual = manual.clone();
    next_manual["idempotency_key"] = json!(Uuid::new_v4());
    let (status, value, _) = request(
        &app,
        "POST",
        "/api/accounting/journals",
        Some(&cookie),
        Some(next_manual.clone()),
    )
    .await;
    success(status, &value);
    let (status,value,_)=request(&app,"POST","/api/accounting/period/close",Some(&cookie),Some(json!({"through":today.to_string(),"reason":"Tutup hari ini untuk uji","idempotency_key":Uuid::new_v4()}))).await;
    success(status, &value);
    // An already committed operation still replays, but a new posting is rejected.
    let (status, value, _) = request(
        &app,
        "POST",
        "/api/accounting/journals",
        Some(&cookie),
        Some(next_manual.clone()),
    )
    .await;
    success(status, &value);
    next_manual["idempotency_key"] = json!(Uuid::new_v4());
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/accounting/journals",
        Some(&cookie),
        Some(next_manual),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, trial, _) = request(
        &app,
        "GET",
        "/api/accounting/trial-balance",
        Some(&cookie),
        None,
    )
    .await;
    success(status, &trial);
    assert_eq!(trial["balanced"], true);
    let (status, _, _) = request(&app, "POST", "/api/auth/logout", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = request(&app, "GET", "/api/auth/me", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
