use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{AppState, app_router, company};

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
    (status, value)
}

fn success(status: StatusCode, body: &Value) {
    assert!(status.is_success(), "{status}: {body}");
}

async fn add_user(pool: &PgPool, role: &str, active: bool) -> (Uuid, String, String) {
    let id = Uuid::new_v4();
    let email = format!("company-{id}@example.test");
    // These fixtures authenticate by session. A valid password is established by the
    // local recovery helper when the recovery test exercises a real login.
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role,active) VALUES($1,'Company Tester',$2,'fixture-unused-hash',$3,$4)")
        .bind(id).bind(&email).bind(role).bind(active).execute(pool).await.unwrap();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '1 day')",
    )
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    (id, email, format!("hf_session={token}"))
}

async fn fixture(pool: &PgPool) -> (Router, Uuid, String, String) {
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Community Before')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let (id, email, cookie) = add_user(pool, "admin", true).await;
    let app = app_router(
        AppState {
            db: pool.clone(),
            setup_token: "company-test-setup-token-over-32-characters".into(),
            secure_cookies: false,
        },
        ".local/company-tests-no-static".into(),
    );
    (app, id, email, cookie)
}

fn profile() -> Value {
    json!({
        "name": "Community Updated", "timezone": "Asia/Makassar",
        "address": "Jl. Merdeka 10", "email": " COMPANY@example.test ",
        "phone": "+62 812 3456", "tax_id": "NPWP-001",
        "order_prefix": "SO-C", "invoice_prefix": "INV-C"
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_profile_permissions_validation_and_financial_timezone_freeze(pool: PgPool) {
    let (app, _admin_id, _email, cookie) = fixture(&pool).await;
    let (_, _, viewer_cookie) = add_user(&pool, "viewer", true).await;
    let (status, _) = request(&app, "GET", "/api/company", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, before) = request(&app, "GET", "/api/company", Some(&viewer_cookie), None).await;
    success(status, &before);
    assert_eq!(before["currency"], "IDR");
    assert_eq!(before["order_prefix"], "SO");
    let (status, _) = request(
        &app,
        "PATCH",
        "/api/company",
        Some(&viewer_cookie),
        Some(profile()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, updated) = request(
        &app,
        "PATCH",
        "/api/company",
        Some(&cookie),
        Some(profile()),
    )
    .await;
    success(status, &updated);
    assert_eq!(updated["name"], "Community Updated");
    assert_eq!(updated["email"], "company@example.test");
    assert_eq!(updated["timezone"], "Asia/Makassar");
    let (_, context) = request(&app, "GET", "/api/auth/me", Some(&cookie), None).await;
    assert_eq!(context["company"], updated);
    for (field, invalid) in [
        ("timezone", "Invalid/Nowhere"),
        ("order_prefix", "lower"),
        ("invoice_prefix", "1INV"),
        ("name", "Invisible\nName"),
        ("email", "not-an-email"),
        ("phone", "+62\n123"),
    ] {
        let mut invalid_profile = profile();
        invalid_profile[field] = json!(invalid);
        let (status, _) = request(
            &app,
            "PATCH",
            "/api/company",
            Some(&cookie),
            Some(invalid_profile),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "validation field {field}");
    }
    let (status, journal) = request(&app, "POST", "/api/accounting/journals", Some(&cookie), Some(json!({
        "description": "Initial equity", "idempotency_key": Uuid::new_v4(),
        "lines": [{"account_code":"1100","debit":"100.00","credit":"0"}, {"account_code":"3000","debit":"0","credit":"100.00"}]
    }))).await;
    success(status, &journal);
    let mut frozen = profile();
    frozen["name"] = json!("Must Not Persist");
    frozen["timezone"] = json!("Asia/Jakarta");
    let (status, _) = request(&app, "PATCH", "/api/company", Some(&cookie), Some(frozen)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, unchanged) = request(&app, "GET", "/api/company", Some(&cookie), None).await;
    assert_eq!(
        unchanged, updated,
        "rejected timezone update must roll back all fields"
    );
    let mut allowed = profile();
    allowed["name"] = json!("Profile After Posting");
    allowed["address"] = json!("");
    allowed["order_prefix"] = json!("SALES");
    let (status, saved) =
        request(&app, "PATCH", "/api/company", Some(&cookie), Some(allowed)).await;
    success(status, &saved);
    assert_eq!(saved["address"], Value::Null);
    assert_eq!(saved["name"], "Profile After Posting");
    let events: Vec<Value> = sqlx::query_scalar("SELECT details FROM audit_events WHERE action='company.profile_updated' ORDER BY created_at")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(events.len(), 2);
    for details in events {
        assert!(details["changed_fields"].is_array());
        assert!(!details.to_string().contains("company@example.test"));
        assert!(!details.to_string().contains("Jl. Merdeka"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_timezone_change_waits_for_the_first_posting(pool: PgPool) {
    let (app, admin_id, _email, cookie) = fixture(&pool).await;
    let mut posting = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *posting)
        .await
        .unwrap();
    let app_clone = app.clone();
    let mut update = tokio::spawn(async move {
        request(
            &app_clone,
            "PATCH",
            "/api/company",
            Some(&cookie),
            Some(profile()),
        )
        .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut update)
            .await
            .is_err(),
        "profile update must wait for the accounting lock"
    );
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'First posting','manual',$1,$2)")
        .bind(id).bind(admin_id).execute(&mut *posting).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1100',100,0),($3,$2,'3000',0,100)")
        .bind(Uuid::new_v4()).bind(id).bind(Uuid::new_v4()).execute(&mut *posting).await.unwrap();
    posting.commit().await.unwrap();
    let (status, _) = tokio::time::timeout(std::time::Duration::from_secs(5), update)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, StatusCode::CONFLICT);
    let timezone: String = sqlx::query_scalar("SELECT timezone FROM company")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(timezone, "Asia/Jakarta");
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_admin_recovery_revokes_sessions_and_rejects_other_accounts(pool: PgPool) {
    let (app, admin_id, email, old_cookie) = fixture(&pool).await;
    let (_, viewer_email, viewer_cookie) = add_user(&pool, "viewer", true).await;
    let (_, inactive_email, _) = add_user(&pool, "admin", false).await;
    let password = "local-recovered-password-strong";
    for rejected_email in [&viewer_email, &inactive_email, "missing@example.test"] {
        let error = company::reset_admin_password(&pool, rejected_email, password.into())
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }
    let error = company::reset_admin_password(&pool, &email, "short".into())
        .await
        .unwrap_err();
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    let (status, _) = request(&app, "GET", "/api/auth/me", Some(&old_cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    company::reset_admin_password(&pool, &email.to_ascii_uppercase(), password.into())
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id=$1")
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    let (status, _) = request(&app, "GET", "/api/auth/me", Some(&old_cookie), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email":email,"password":password})),
    )
    .await;
    success(status, &body);
    assert_eq!(body["user"]["role"], "admin");
    let (status, _) = request(&app, "GET", "/api/auth/me", Some(&viewer_cookie), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "recovery must leave other users' sessions intact"
    );
    let events: Vec<Value> = sqlx::query_scalar(
        "SELECT details FROM audit_events WHERE action='user.password_reset_local'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events, vec![json!({})]);
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert_ne!(hash, password);
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_prefix_updates_preserve_existing_numbers_and_sequence(pool: PgPool) {
    let (app, _admin_id, _email, cookie) = fixture(&pool).await;
    let (status, customer) = request(
        &app,
        "POST",
        "/api/customers",
        Some(&cookie),
        Some(json!({"name":"Prefix Customer"})),
    )
    .await;
    success(status, &customer);
    let (status, product) = request(
        &app,
        "POST",
        "/api/products",
        Some(&cookie),
        Some(json!({
            "sku":"PREFIX-PRODUCT", "name":"Prefix product", "unit":"pcs", "price":"100"
        })),
    )
    .await;
    success(status, &product);
    let (status, receipt) = request(&app, "POST", "/api/inventory/receipts", Some(&cookie), Some(json!({
        "product_id":product["id"], "quantity":"2", "unit_cost":"10", "idempotency_key":Uuid::new_v4()
    }))).await;
    success(status, &receipt);
    let new_order = || {
        json!({
            "customer_id":customer["id"], "idempotency_key":Uuid::new_v4(),
            "items":[{"product_id":product["id"],"quantity":"1","unit_price":"100"}]
        })
    };
    let (status, first) = request(
        &app,
        "POST",
        "/api/orders",
        Some(&cookie),
        Some(new_order()),
    )
    .await;
    success(status, &first);
    let first_invoice = confirm_and_invoice(&app, &cookie, &first).await;
    let mut new_profile = profile();
    new_profile["timezone"] = json!("Asia/Jakarta");
    let (status, _) = request(
        &app,
        "PATCH",
        "/api/company",
        Some(&cookie),
        Some(new_profile),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, second) = request(
        &app,
        "POST",
        "/api/orders",
        Some(&cookie),
        Some(new_order()),
    )
    .await;
    success(status, &second);
    let first_number = first["number"].as_str().unwrap();
    let second_number = second["number"].as_str().unwrap();
    assert!(first_number.starts_with("SO-"));
    assert!(second_number.starts_with("SO-C-"));
    let first_sequence: u64 = first_number.rsplit('-').next().unwrap().parse().unwrap();
    let second_sequence: u64 = second_number.rsplit('-').next().unwrap().parse().unwrap();
    assert!(second_sequence > first_sequence);
    let (_, old_order) = request(
        &app,
        "GET",
        &format!("/api/orders/{}", first["id"].as_str().unwrap()),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(old_order["number"], first["number"]);
    let second_invoice = confirm_and_invoice(&app, &cookie, &second).await;
    let first_invoice_number = first_invoice["number"].as_str().unwrap();
    let second_invoice_number = second_invoice["number"].as_str().unwrap();
    assert!(first_invoice_number.starts_with("INV-"));
    assert!(second_invoice_number.starts_with("INV-C-"));
    let first_invoice_sequence: u64 = first_invoice_number
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let second_invoice_sequence: u64 = second_invoice_number
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(second_invoice_sequence > first_invoice_sequence);
    let stored_number: String = sqlx::query_scalar("SELECT number FROM invoices WHERE id=$1")
        .bind(Uuid::parse_str(first_invoice["id"].as_str().unwrap()).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_number, first_invoice_number);
}

async fn confirm_and_invoice(app: &Router, cookie: &str, order: &Value) -> Value {
    let order_path = format!("/api/orders/{}", order["id"].as_str().unwrap());
    let (status, confirmed) = request(
        app,
        "POST",
        &format!("{order_path}/confirm"),
        Some(cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &confirmed);
    let (status, shipped) = request(
        app,
        "POST",
        &format!("{order_path}/ship"),
        Some(cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &shipped);
    let (status, invoice) = request(
        app,
        "POST",
        &format!("{order_path}/invoice"),
        Some(cookie),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    success(status, &invoice);
    invoice
}
