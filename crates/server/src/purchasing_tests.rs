use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    app: &Router,
    cookie: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
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

async fn call(app: &Router, cookie: &str, method: &str, path: &str, body: Option<Value>) -> Value {
    let (status, value) = request(app, cookie, method, path, body).await;
    assert!(status.is_success(), "{method} {path}: {status}: {value}");
    value
}

async fn actor(pool: &PgPool, role: &str) -> String {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Purchasing tester',$2,'fixture-unused',$3)")
        .bind(id).bind(format!("{id}@example.test")).bind(role).execute(pool).await.unwrap();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '1 day')",
    )
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    format!("hf_session={token}")
}

async fn fixture(pool: &PgPool) -> (Router, String) {
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Purchasing test')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let cookie = actor(pool, "admin").await;
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "purchasing-fixture-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        ".local/purchasing-no-static".into(),
    );
    (app, cookie)
}

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap()
}

fn amount(value: &Value, key: &str) -> Decimal {
    value[key].as_str().unwrap().parse().unwrap()
}

fn supplier_body(code: &str) -> Value {
    json!({"code":code,"name":"PT Supplier Awal","email":"supplier@example.test","phone":"0812345","address":"Jalan Uji 1\nJakarta","idempotency_key":Uuid::new_v4()})
}

async fn supplier(app: &Router, cookie: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/suppliers",
        Some(supplier_body("SUP-001")),
    )
    .await
}

async fn product(app: &Router, cookie: &str, sku: &str, kind: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/products",
        Some(json!({"sku":sku,"name":"Kopi awal","kind":kind,"unit":"kg","price":"80000"})),
    )
    .await
}

fn order_body(supplier: &Value, product: &Value) -> Value {
    json!({"supplier_id":supplier["id"],"expected_date":"2026-12-31","notes":"Kirim ke gudang\nHubungi admin","items":[{"product_id":product["id"],"quantity":"1.125","unit_cost":"12.3456"}],"idempotency_key":Uuid::new_v4()})
}

async fn order(app: &Router, cookie: &str, supplier: &Value, product: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/purchase-orders",
        Some(order_body(supplier, product)),
    )
    .await
}

fn supplier_update(supplier: &Value, active: bool) -> Value {
    json!({"code":supplier["code"],"name":supplier["name"],"email":supplier["email"],"phone":supplier["phone"],"address":supplier["address"],"active":active,"version":supplier["version"],"idempotency_key":Uuid::new_v4()})
}

fn order_action(order: &Value) -> Value {
    json!({"version":order["version"],"idempotency_key":Uuid::new_v4()})
}

fn cancel_action(order: &Value) -> Value {
    json!({"version":order["version"],"reason":"Supplier tidak dapat memenuhi pesanan","idempotency_key":Uuid::new_v4()})
}

async fn assert_no_posting(pool: &PgPool) {
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM journal_entries),(SELECT count(*) FROM journal_lines),(SELECT count(*) FROM inventory_movements)")
        .fetch_one(pool).await.unwrap();
    assert_eq!(
        counts,
        (0, 0, 0),
        "A purchase order is a commitment; receipt and payment are separate stages"
    );
    let changed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM products WHERE quantity<>0 OR inventory_value<>0 OR average_cost<>0",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(changed, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_supplier_contact_updates_are_versioned_and_audited(pool: PgPool) {
    let (app, cookie) = fixture(&pool).await;
    let created = call(
        &app,
        &cookie,
        "POST",
        "/api/suppliers",
        Some(supplier_body(" sup-001 ")),
    )
    .await;
    assert_eq!(created["code"], "SUP-001");
    assert_eq!(created["active"], true);
    assert_eq!(created["version"], 1);
    let path = format!("/api/suppliers/{}", id(&created));
    let mut edit = supplier_update(&created, false);
    edit["name"] = json!("  Supplier Baru  ");
    edit["email"] = json!("   ");
    edit["phone"] = json!("");
    edit["address"] = json!("Alamat baru\nBandung");
    let updated = call(&app, &cookie, "PUT", &path, Some(edit.clone())).await;
    assert_eq!(updated["name"], "Supplier Baru");
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["active"], false);
    assert!(updated["email"].is_null());
    assert!(updated["phone"].is_null());
    assert_eq!(updated["address"], "Alamat baru\nBandung");
    assert_eq!(call(&app, &cookie, "PUT", &path, Some(edit)).await, updated);
    let stale = supplier_update(&created, true);
    assert_eq!(
        request(&app, &cookie, "PUT", &path, Some(stale)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/suppliers",
            Some(supplier_body("sup-001"))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let suppliers = call(&app, &cookie, "GET", "/api/suppliers", None).await;
    assert_eq!(suppliers["items"], json!([updated]));
    let audit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE resource_id=$1")
            .bind(Uuid::parse_str(id(&created)).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        audit_count, 2,
        "Retries and rejected stale updates must not create new audit events"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_order_lifecycle_keeps_snapshots_and_does_not_post_stock_or_money(pool: PgPool) {
    let (app, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "KOPI-001", "stock").await;
    let draft = order(&app, &cookie, &supplier, &product).await;
    let path = format!("/api/purchase-orders/{}", id(&draft));
    assert_eq!(draft["status"], "draft");
    assert_eq!(draft["version"], 1);
    assert!(draft["number"].as_str().unwrap().starts_with("PO"));
    assert_eq!(amount(&draft, "total"), "13.89".parse::<Decimal>().unwrap());
    assert_eq!(draft["items"][0]["product_sku"], "KOPI-001");
    assert_eq!(draft["items"][0]["product_name"], "Kopi awal");
    assert_eq!(draft["items"][0]["unit"], "kg");
    assert_eq!(draft["supplier_name"], "PT Supplier Awal");
    let mut edit = order_body(&supplier, &product);
    edit["version"] = draft["version"].clone();
    edit["items"][0]["quantity"] = json!("2.125");
    edit["expected_date"] = Value::Null;
    let edited = call(&app, &cookie, "PUT", &path, Some(edit.clone())).await;
    assert_eq!(edited["version"], 2);
    assert_eq!(edited["number"], draft["number"]);
    assert_eq!(
        amount(&edited, "total"),
        "26.23".parse::<Decimal>().unwrap()
    );
    assert!(edited["expected_date"].is_null());
    assert_eq!(call(&app, &cookie, "PUT", &path, Some(edit)).await, edited);
    call(&app,&cookie,"PATCH",&format!("/api/products/{}",id(&product)),Some(json!({"sku":"KOPI-BARU","name":"Kopi diganti","kind":"stock","unit":"kotak","price":"90000"}))).await;
    let mut supplier_edit = supplier_update(&supplier, true);
    supplier_edit["code"] = json!("SUP-BARU");
    supplier_edit["name"] = json!("Supplier diganti");
    call(
        &app,
        &cookie,
        "PUT",
        &format!("/api/suppliers/{}", id(&supplier)),
        Some(supplier_edit),
    )
    .await;
    let unchanged = call(&app, &cookie, "GET", &path, None).await;
    assert_eq!(
        unchanged, edited,
        "Master data edits must preserve the order's reviewed snapshots"
    );
    let confirm_body = order_action(&edited);
    let confirmed = call(
        &app,
        &cookie,
        "POST",
        &format!("{path}/confirm"),
        Some(confirm_body.clone()),
    )
    .await;
    assert_eq!(confirmed["status"], "confirmed");
    assert_eq!(confirmed["version"], 3);
    assert_eq!(confirmed["items"], edited["items"]);
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &format!("{path}/confirm"),
            Some(confirm_body)
        )
        .await,
        confirmed
    );
    let mut prohibited = order_body(&supplier, &product);
    prohibited["version"] = confirmed["version"].clone();
    assert_eq!(
        request(&app, &cookie, "PUT", &path, Some(prohibited))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let cancel_body = cancel_action(&confirmed);
    let cancelled = call(
        &app,
        &cookie,
        "POST",
        &format!("{path}/cancel"),
        Some(cancel_body.clone()),
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["version"], 4);
    assert_eq!(cancelled["cancellation_reason"], cancel_body["reason"]);
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &format!("{path}/cancel"),
            Some(cancel_body)
        )
        .await,
        cancelled
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("{path}/confirm"),
            Some(order_action(&cancelled))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("{path}/cancel"),
            Some(cancel_action(&cancelled))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let audit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE resource_id=$1")
            .bind(Uuid::parse_str(id(&draft)).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit_count, 4);
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_rejects_invalid_contacts_dates_lines_and_decimal_values(pool: PgPool) {
    let (app, cookie) = fixture(&pool).await;
    for (field, value) in [
        ("code", json!("")),
        ("code", json!("X".repeat(65))),
        ("name", json!("\n")),
        ("name", json!("X".repeat(201))),
        ("email", json!("x".repeat(255))),
        ("phone", json!("x".repeat(65))),
        ("address", json!("x".repeat(1001))),
    ] {
        let mut body = supplier_body("INVALID");
        body[field] = value;
        let (status, result) = request(&app, &cookie, "POST", "/api/suppliers", Some(body)).await;
        assert!(status.is_client_error(), "{field}: {status}: {result}");
    }
    let supplier = supplier(&app, &cookie).await;
    let stock = product(&app, &cookie, "STOCK", "stock").await;
    let mut invalid_bodies = Vec::new();
    for (field, value) in [
        ("expected_date", json!("2026-02-30")),
        ("expected_date", json!("2026-13-01")),
        ("expected_date", json!("2026-12-31T12:00:00Z")),
        ("expected_date", json!("-0001-01-01")),
        ("expected_date", json!("+10000-01-01")),
        ("notes", json!("n".repeat(2001))),
        ("items", json!([])),
        (
            "items",
            json!([{"product_id":Uuid::new_v4(),"quantity":"1","unit_cost":"10"}]),
        ),
        ("supplier_id", json!(Uuid::new_v4())),
    ] {
        let mut body = order_body(&supplier, &stock);
        body[field] = value;
        invalid_bodies.push(body);
    }
    let base = order_body(&supplier, &stock);
    let mut duplicate = base.clone();
    duplicate["items"] = json!([base["items"][0], base["items"][0]]);
    invalid_bodies.push(duplicate);
    let mut too_many = base.clone();
    too_many["items"] = Value::Array(vec![base["items"][0].clone(); 101]);
    invalid_bodies.push(too_many);
    for (field, value) in [
        ("quantity", "0"),
        ("quantity", "-1"),
        ("quantity", "1.0001"),
        ("quantity", "1000000001"),
        ("unit_cost", "0"),
        ("unit_cost", "-1"),
        ("unit_cost", "1.00001"),
        ("unit_cost", "1000000001"),
        ("unit_cost", "NaN"),
    ] {
        let mut body = order_body(&supplier, &stock);
        body["items"][0][field] = json!(value);
        invalid_bodies.push(body);
    }
    let mut too_valuable = base.clone();
    too_valuable["items"][0]["quantity"] = json!("1000000000");
    too_valuable["items"][0]["unit_cost"] = json!("1000000000");
    invalid_bodies.push(too_valuable);
    let stock2 = product(&app, &cookie, "STOCK2", "stock").await;
    let mut aggregate = base.clone();
    aggregate["items"] = json!([
        {"product_id":stock["id"],"quantity":"1000000","unit_cost":"700000"},
        {"product_id":stock2["id"],"quantity":"1000000","unit_cost":"700000"}
    ]);
    invalid_bodies.push(aggregate);
    let mut rounded_zero = base.clone();
    rounded_zero["items"][0]["quantity"] = json!("0.001");
    rounded_zero["items"][0]["unit_cost"] = json!("0.0001");
    invalid_bodies.push(rounded_zero);
    for body in invalid_bodies {
        let (status, result) = request(
            &app,
            &cookie,
            "POST",
            "/api/purchase-orders",
            Some(body.clone()),
        )
        .await;
        assert!(
            status.is_client_error(),
            "Invalid purchase input accepted or failed internally: {body}: {status}: {result}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM purchase_orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "Invalid documents must leave no drafts or numbers behind"
    );
    let valid = order(&app, &cookie, &supplier, &stock).await;
    for reason in ["ab".to_owned(), "a".repeat(1001)] {
        let mut body = cancel_action(&valid);
        body["reason"] = json!(reason);
        assert!(
            request(
                &app,
                &cookie,
                "POST",
                &format!("/api/purchase-orders/{}/cancel", id(&valid)),
                Some(body)
            )
            .await
            .0
            .is_client_error()
        );
    }
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_roles_can_read_but_only_admin_and_warehouse_can_write(pool: PgPool) {
    let (app, admin) = fixture(&pool).await;
    let supplier = supplier(&app, &admin).await;
    let product = product(&app, &admin, "ROLE-STOCK", "stock").await;
    let draft = order(&app, &admin, &supplier, &product).await;
    let path = format!("/api/purchase-orders/{}", id(&draft));
    for path in ["/api/suppliers", "/api/purchase-orders", &path] {
        assert_eq!(
            request(&app, "", "GET", path, None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    for role in ["sales", "finance", "viewer"] {
        let cookie = actor(&pool, role).await;
        for path in ["/api/suppliers", "/api/purchase-orders", &path] {
            assert_eq!(
                request(&app, &cookie, "GET", path, None).await.0,
                StatusCode::OK,
                "{role} can read {path}"
            );
        }
        let mut update = order_body(&supplier, &product);
        update["version"] = draft["version"].clone();
        for (method, path, body) in [
            (
                "POST",
                "/api/suppliers".to_owned(),
                supplier_body(&format!("DENIED-{role}")),
            ),
            (
                "PUT",
                format!("/api/suppliers/{}", id(&supplier)),
                supplier_update(&supplier, true),
            ),
            (
                "POST",
                "/api/purchase-orders".to_owned(),
                order_body(&supplier, &product),
            ),
            ("PUT", path.clone(), update),
            ("POST", format!("{path}/confirm"), order_action(&draft)),
            ("POST", format!("{path}/cancel"), cancel_action(&draft)),
        ] {
            let (status, value) = request(&app, &cookie, method, &path, Some(body)).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{role} {method} {path}: {value}"
            );
        }
    }
    let warehouse = actor(&pool, "warehouse").await;
    let warehouse_supplier = call(
        &app,
        &warehouse,
        "POST",
        "/api/suppliers",
        Some(supplier_body("WH-SUP")),
    )
    .await;
    let updated = call(
        &app,
        &warehouse,
        "PUT",
        &format!("/api/suppliers/{}", id(&warehouse_supplier)),
        Some(supplier_update(&warehouse_supplier, true)),
    )
    .await;
    let warehouse_order = order(&app, &warehouse, &updated, &product).await;
    let warehouse_path = format!("/api/purchase-orders/{}", id(&warehouse_order));
    let mut edit = order_body(&updated, &product);
    edit["version"] = warehouse_order["version"].clone();
    let edited = call(&app, &warehouse, "PUT", &warehouse_path, Some(edit)).await;
    let confirmed = call(
        &app,
        &warehouse,
        "POST",
        &format!("{warehouse_path}/confirm"),
        Some(order_action(&edited)),
    )
    .await;
    let cancelled = call(
        &app,
        &warehouse,
        "POST",
        &format!("{warehouse_path}/cancel"),
        Some(cancel_action(&confirmed)),
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_idempotency_is_bound_to_actor_body_and_operation_under_concurrent_retries(
    pool: PgPool,
) {
    let (app, cookie) = fixture(&pool).await;
    let body = supplier_body("RETRY-SUP");
    let (first, second) = tokio::join!(
        request(&app, &cookie, "POST", "/api/suppliers", Some(body.clone())),
        request(&app, &cookie, "POST", "/api/suppliers", Some(body.clone()))
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second, first);
    let supplier = first.1;
    let other = actor(&pool, "warehouse").await;
    assert_eq!(
        request(&app, &other, "POST", "/api/suppliers", Some(body.clone()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut changed = body.clone();
    changed["name"] = json!("A different supplier");
    assert_eq!(
        request(&app, &cookie, "POST", "/api/suppliers", Some(changed))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let product = product(&app, &cookie, "RETRY-STOCK", "stock").await;
    let mut cross_operation = order_body(&supplier, &product);
    cross_operation["idempotency_key"] = body["idempotency_key"].clone();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/purchase-orders",
            Some(cross_operation)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let body = order_body(&supplier, &product);
    let (first, second) = tokio::join!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/purchase-orders",
            Some(body.clone())
        ),
        request(
            &app,
            &cookie,
            "POST",
            "/api/purchase-orders",
            Some(body.clone())
        )
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second, first);
    let draft = first.1;
    assert_eq!(
        request(
            &app,
            &other,
            "POST",
            "/api/purchase-orders",
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut changed = body;
    changed["items"][0]["quantity"] = json!("2");
    assert_eq!(
        request(&app, &cookie, "POST", "/api/purchase-orders", Some(changed))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let path = format!("/api/purchase-orders/{}/confirm", id(&draft));
    let confirm = order_action(&draft);
    let (first, second) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(confirm.clone())),
        request(&app, &cookie, "POST", &path, Some(confirm.clone()))
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second, first);
    assert_eq!(first.1["version"], 2);
    let mut stale_body = confirm;
    stale_body["version"] = json!(2);
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(stale_body))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM suppliers),(SELECT count(*) FROM purchase_orders),(SELECT count(*) FROM audit_events WHERE action='purchase_order.created'),(SELECT count(*) FROM audit_events WHERE action='purchase_order.confirmed')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, 1));
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_optimistic_versions_resolve_competing_edits_and_transitions_once(pool: PgPool) {
    let (app, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "VERSION-STOCK", "stock").await;
    let draft = order(&app, &cookie, &supplier, &product).await;
    let path = format!("/api/purchase-orders/{}", id(&draft));
    let mut edit_a = order_body(&supplier, &product);
    edit_a["version"] = draft["version"].clone();
    edit_a["notes"] = json!("Revision A");
    let mut edit_b = order_body(&supplier, &product);
    edit_b["version"] = draft["version"].clone();
    edit_b["notes"] = json!("Revision B");
    let (a, b) = tokio::join!(
        request(&app, &cookie, "PUT", &path, Some(edit_a)),
        request(&app, &cookie, "PUT", &path, Some(edit_b))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT),
        "Competing edits: {a:?}, {b:?}"
    );
    let edited = if a.0 == StatusCode::OK { a.1 } else { b.1 };
    assert_eq!(edited["version"], 2);
    let confirm_path = format!("{path}/confirm");
    let cancel_path = format!("{path}/cancel");
    let (confirmed, cancelled) = tokio::join!(
        request(
            &app,
            &cookie,
            "POST",
            &confirm_path,
            Some(order_action(&edited))
        ),
        request(
            &app,
            &cookie,
            "POST",
            &cancel_path,
            Some(cancel_action(&edited))
        )
    );
    assert!(
        (confirmed.0 == StatusCode::OK && cancelled.0 == StatusCode::CONFLICT)
            || (cancelled.0 == StatusCode::OK && confirmed.0 == StatusCode::CONFLICT),
        "Competing transitions: {confirmed:?}, {cancelled:?}"
    );
    let winner = if confirmed.0 == StatusCode::OK {
        confirmed.1
    } else {
        cancelled.1
    };
    assert_eq!(winner["version"], 3);
    assert_eq!(call(&app, &cookie, "GET", &path, None).await, winner);
    let audit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE resource_id=$1")
            .bind(Uuid::parse_str(id(&draft)).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        audit_count, 3,
        "One created, one edited, one transition; rejected concurrent requests leave no events"
    );
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_inactive_suppliers_and_referenced_products_keep_document_history_safe(
    pool: PgPool,
) {
    let (app, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "REFERENCED", "stock").await;
    let draft = order(&app, &cookie, &supplier, &product).await;
    let path = format!("/api/purchase-orders/{}", id(&draft));
    let supplier_path = format!("/api/suppliers/{}", id(&supplier));
    let inactive = call(
        &app,
        &cookie,
        "PUT",
        &supplier_path,
        Some(supplier_update(&supplier, false)),
    )
    .await;
    assert_eq!(
        inactive["active"], false,
        "A used supplier can be deactivated while history remains readable"
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/purchase-orders",
            Some(order_body(&inactive, &product))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("{path}/confirm"),
            Some(order_action(&draft))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let product_path = format!("/api/products/{}", id(&product));
    assert_eq!(request(&app,&cookie,"PATCH",&product_path,Some(json!({"sku":"REFERENCED","name":"Kopi awal","kind":"service","unit":"kg","price":"80000"}))).await.0,StatusCode::CONFLICT);
    assert_eq!(
        request(&app, &cookie, "DELETE", &product_path, None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let cancelled = call(
        &app,
        &cookie,
        "POST",
        &format!("{path}/cancel"),
        Some(cancel_action(&draft)),
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(
        request(&app, &cookie, "DELETE", &product_path, None)
            .await
            .0,
        StatusCode::CONFLICT,
        "Cancelled purchase documents still retain their product references"
    );
    let active = call(
        &app,
        &cookie,
        "PUT",
        &supplier_path,
        Some(supplier_update(&inactive, true)),
    )
    .await;
    let next = order(&app, &cookie, &active, &product).await;
    assert_ne!(
        next["number"], draft["number"],
        "Cancelled numbers must not be reused"
    );
    assert_no_posting(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_order_numbers_preserve_digits_beyond_the_initial_padding(pool: PgPool) {
    let (app, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "NUMBER-STOCK", "stock").await;
    sqlx::query("SELECT setval('purchase_order_number',999999,true)")
        .execute(&pool)
        .await
        .unwrap();
    let millionth = order(&app, &cookie, &supplier, &product).await;
    let following = order(&app, &cookie, &supplier, &product).await;
    assert_eq!(millionth["number"], "PO-1000000");
    assert_eq!(following["number"], "PO-1000001");
}
