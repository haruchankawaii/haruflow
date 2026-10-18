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
                .body(
                    body.map(|body| Body::from(body.to_string()))
                        .unwrap_or_default(),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn call(app: &Router, cookie: &str, method: &str, path: &str, body: Option<Value>) -> Value {
    let (status, value) = request(app, cookie, method, path, body).await;
    assert!(status.is_success(), "{method} {path}: {status}: {value}");
    value
}

async fn user(pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Correction tester',$2,'fixture-unused',$3)")
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
    (id, format!("hf_session={token}"))
}

async fn fixture(pool: &PgPool) -> (Router, Uuid, String) {
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Document correction test')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let (id, cookie) = user(pool, "admin").await;
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "correction-test-token-over-32-characters".into(),
            secure_cookies: false,
        },
        ".local/corrections-no-static".into(),
    );
    (app, id, cookie)
}

fn reverse_body() -> Value {
    json!({"reason":"Correction of an input mistake","idempotency_key":Uuid::new_v4()})
}
fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap()
}
fn amount(value: &Value, key: &str) -> Decimal {
    value[key].as_str().unwrap().parse().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}

async fn manual(app: &Router, cookie: &str) -> Value {
    call(app,cookie,"POST","/api/accounting/journals",Some(json!({"description":"Manual journal with three lines",
        "lines":[{"account_code":"1100","debit":"100.01","credit":"0"},{"account_code":"3000","debit":"0","credit":"60"},{"account_code":"4000","debit":"0","credit":"40.01"}],
        "idempotency_key":Uuid::new_v4()}))).await
}
async fn product(app: &Router, cookie: &str) -> Value {
    call(app,cookie,"POST","/api/products",Some(json!({"sku":format!("COR-{}",Uuid::new_v4().simple()),"name":"Correction stock","price":"80"}))).await
}
async fn receipt(app: &Router, cookie: &str, product: &Value, quantity: &str, cost: &str) -> Value {
    call(app,cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":product["id"],"quantity":quantity,"unit_cost":cost,"idempotency_key":Uuid::new_v4()}))).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn manual_reversal_exactly_inverts_every_line_and_keeps_audit_links(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let original = manual(&app, &cookie).await;
    let path = format!("/api/accounting/journals/{}/reverse", id(&original));
    let body = reverse_body();
    let reversed = call(&app, &cookie, "POST", &path, Some(body.clone())).await;
    assert_eq!(reversed["source_id"], original["id"]);
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(body.clone())).await,
        reversed
    );
    let mut changed = body;
    changed["reason"] = json!("Different correction reason");
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(changed)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(reverse_body()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let journals = call(&app, &cookie, "GET", "/api/accounting/journals", None).await;
    let source = journals["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == original["id"])
        .unwrap();
    let inverse = journals["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == reversed["journal_id"])
        .unwrap();
    assert_eq!(source["can_reverse"], false);
    assert_eq!(source["reversal"]["id"], reversed["id"]);
    assert_eq!(
        inverse["reversed_source"],
        json!({"source_type":"manual","source_id":original["id"]})
    );
    assert_eq!(inverse["can_reverse"], false);
    assert_eq!(inverse["lines"].as_array().unwrap().len(), 3);
    for line in source["lines"].as_array().unwrap() {
        let inverted = inverse["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["account_code"] == line["account_code"])
            .unwrap();
        assert_eq!(line["debit"], inverted["credit"]);
        assert_eq!(line["credit"], inverted["debit"]);
    }
    let journal_id: Uuid = id(&original).parse().unwrap();
    assert!(
        sqlx::query("DELETE FROM journal_entries WHERE id=$1")
            .bind(journal_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE journal_lines SET debit=debit WHERE journal_id=$1")
            .bind(journal_id)
            .execute(&pool)
            .await
            .is_err()
    );
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action='journal.reversed' AND resource_id=$1",
    )
    .bind(journal_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit, 1);
    let balance = call(&app, &cookie, "GET", "/api/accounting/trial-balance", None).await;
    assert_eq!(amount(&balance, "total_debit"), Decimal::ZERO);
    assert_eq!(balance["balanced"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn correction_permissions_source_types_and_closed_period_are_atomic(pool: PgPool) {
    let (app, _, admin) = fixture(&pool).await;
    let (_, finance) = user(&pool, "finance").await;
    let (_, warehouse) = user(&pool, "warehouse").await;
    let (_, sales) = user(&pool, "sales").await;
    let journal = manual(&app, &admin).await;
    let stock = product(&app, &admin).await;
    let receipt = receipt(&app, &admin, &stock, "3", "5").await;
    let journal_path = format!("/api/accounting/journals/{}/reverse", id(&journal));
    let receipt_path = format!("/api/inventory/receipts/{}/reverse", id(&receipt));
    for (cookie, path) in [
        (&warehouse, &journal_path),
        (&sales, &journal_path),
        (&finance, &receipt_path),
        (&sales, &receipt_path),
    ] {
        assert_eq!(
            request(&app, cookie, "POST", path, Some(reverse_body()))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    let receipt_journal: Uuid =
        sqlx::query_scalar("SELECT id FROM journal_entries WHERE source_type='receipt'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        request(
            &app,
            &finance,
            "POST",
            &format!("/api/accounting/journals/{receipt_journal}/reverse"),
            Some(reverse_body())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            &finance,
            "POST",
            &journal_path,
            Some(json!({"reason":"x","idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton")
        .execute(&pool).await.unwrap();
    for (cookie, path) in [(&finance, &journal_path), (&warehouse, &receipt_path)] {
        assert_eq!(
            request(&app, cookie, "POST", path, Some(reverse_body()))
                .await
                .0,
            StatusCode::CONFLICT
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM posting_reversals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        call(&app, &admin, "GET", "/api/inventory/movements", None).await["items"][0]["can_reverse"],
        false
    );
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date-1 WHERE singleton")
        .execute(&pool).await.unwrap();
    let reversed = call(&app, &finance, "POST", &journal_path, Some(reverse_body())).await;
    let today:bool=sqlx::query_scalar("SELECT (created_at AT TIME ZONE (SELECT timezone FROM company))::date=(now() AT TIME ZONE (SELECT timezone FROM company))::date FROM journal_entries WHERE id=$1")
        .bind(Uuid::parse_str(reversed["journal_id"].as_str().unwrap()).unwrap()).fetch_one(&pool).await.unwrap();
    assert!(today);
    call(
        &app,
        &warehouse,
        "POST",
        &receipt_path,
        Some(reverse_body()),
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn latest_receipt_restores_exact_snapshot_including_shipment_rounding(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie).await;
    receipt(&app, &cookie, &stock, "3", "3.3333").await;
    // A partial stock removal leaves 6.67 carrying value and the original
    // 3.3333 average, rather than recalculating a new 3.3350 average.
    call(&app,&cookie,"POST","/api/inventory/adjustments",Some(json!({"product_id":stock["id"],"quantity":"-1","reason":"Damaged unit","idempotency_key":Uuid::new_v4()}))).await;
    let added = receipt(&app, &cookie, &stock, "1", "5").await;
    let before = call(&app, &cookie, "GET", "/api/inventory/movements", None).await;
    assert_eq!(before["items"][0]["can_reverse"], true);
    let body = reverse_body();
    let path = format!("/api/inventory/receipts/{}/reverse", id(&added));
    let reversed = call(&app, &cookie, "POST", &path, Some(body.clone())).await;
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(body)).await,
        reversed
    );
    let products = call(&app, &cookie, "GET", "/api/products", None).await;
    assert_eq!(amount(&products["items"][0], "quantity"), d("2"));
    assert_eq!(amount(&products["items"][0], "inventory_value"), d("6.67"));
    assert_eq!(amount(&products["items"][0], "average_cost"), d("3.3333"));
    let movements = call(&app, &cookie, "GET", "/api/inventory/movements", None).await;
    assert_eq!(movements["items"][0]["kind"], "receipt_reversal");
    assert_eq!(movements["items"][0]["reverses_movement_id"], added["id"]);
    assert_eq!(amount(&movements["items"][0], "quantity"), d("-1"));
    assert_eq!(amount(&movements["items"][0], "value"), d("5"));
    let original = movements["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == added["id"])
        .unwrap();
    assert_eq!(original["reversal"]["journal_id"], reversed["journal_id"]);
    let inventory: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1300'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inventory, d("6.67"));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action='inventory.receipt_reversed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn later_movements_block_receipt_reversal_even_when_zero_valued_or_reversed(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie).await;
    let first = receipt(&app, &cookie, &stock, "1", "0.01").await;
    call(&app,&cookie,"POST","/api/inventory/adjustments",Some(json!({"product_id":stock["id"],"quantity":"-0.001","reason":"Tiny count correction","idempotency_key":Uuid::new_v4()}))).await;
    let path = format!("/api/inventory/receipts/{}/reverse", id(&first));
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(reverse_body()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let second = receipt(&app, &cookie, &stock, "1", "2").await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/inventory/receipts/{}/reverse", id(&second)),
        Some(reverse_body()),
    )
    .await;
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(reverse_body()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let movements = call(&app, &cookie, "GET", "/api/inventory/movements", None).await;
    let original = movements["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == first["id"])
        .unwrap();
    assert_eq!(original["can_reverse"], false);
    assert!(
        original["reversal_blocked_reason"]
            .as_str()
            .unwrap()
            .contains("mutasi stok")
    );
    assert_eq!(movements["items"].as_array().unwrap().len(), 4);
    let adjustment = movements["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["kind"] == "adjustment")
        .unwrap();
    assert_eq!(amount(adjustment, "value"), Decimal::ZERO);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("/api/inventory/receipts/{}/reverse", id(adjustment)),
            Some(reverse_body())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_retries_do_not_duplicate_corrections_and_keys_bind_actor(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let first = manual(&app, &cookie).await;
    let path = format!("/api/accounting/journals/{}/reverse", id(&first));
    let body = reverse_body();
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(body.clone())),
        request(&app, &cookie, "POST", &path, Some(body.clone()))
    );
    assert_eq!(a.0, StatusCode::OK);
    assert_eq!(a, b);
    let (_, finance) = user(&pool, "finance").await;
    assert_eq!(
        request(&app, &finance, "POST", &path, Some(body)).await.0,
        StatusCode::CONFLICT
    );
    let second = manual(&app, &cookie).await;
    let path = format!("/api/accounting/journals/{}/reverse", id(&second));
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(reverse_body())),
        request(&app, &cookie, "POST", &path, Some(reverse_body()))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
    );
    let stock = product(&app, &cookie).await;
    let receipt = receipt(&app, &cookie, &stock, "2", "1000000000").await;
    let path = format!("/api/inventory/receipts/{}/reverse", id(&receipt));
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(reverse_body())),
        request(&app, &cookie, "POST", &path, Some(reverse_body()))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
    );
    let counts=sqlx::query("SELECT (SELECT count(*) FROM posting_reversals) reversals,(SELECT count(*) FROM inventory_movements WHERE kind='receipt_reversal') movements")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts.get::<i64, _>("reversals"), 3);
    assert_eq!(counts.get::<i64, _>("movements"), 1);
    let products = call(&app, &cookie, "GET", "/api/products", None).await;
    assert_eq!(amount(&products["items"][0], "quantity"), Decimal::ZERO);
    assert_eq!(
        amount(&products["items"][0], "inventory_value"),
        Decimal::ZERO
    );
}

#[sqlx::test(migrations = false)]
async fn legacy_receipts_keep_canonical_history_but_cannot_guess_pre_upgrade_costs(pool: PgPool) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|migration| migration.version <= 8)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    let (app, admin, cookie) = fixture(&pool).await;
    let product = Uuid::new_v4();
    let legacy_receipt = Uuid::new_v4();
    sqlx::query("INSERT INTO products(id,sku,name,price,quantity,average_cost,inventory_value) VALUES($1,'LEGACY-COR','Legacy correction product',80,3,3.3333,10)")
        .bind(product).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,created_by) VALUES($1,$2,'receipt',3,3.3333,10,$3)")
        .bind(legacy_receipt).bind(product).bind(admin).execute(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let movements = call(&app, &cookie, "GET", "/api/inventory/movements", None).await;
    assert_eq!(movements["items"][0]["id"], json!(legacy_receipt));
    assert_eq!(movements["items"][0]["can_reverse"], false);
    assert!(
        movements["items"][0]["reversal_blocked_reason"]
            .as_str()
            .unwrap()
            .contains("sebelum upgrade")
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("/api/inventory/receipts/{legacy_receipt}/reverse"),
            Some(reverse_body())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let fresh = receipt(&app, &cookie, &json!({"id":product}), "1", "5").await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/inventory/receipts/{}/reverse", id(&fresh)),
        Some(reverse_body()),
    )
    .await;
    let row = sqlx::query("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
        .bind(product)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Decimal, _>("quantity"), d("3"));
    assert_eq!(row.get::<Decimal, _>("average_cost"), d("3.3333"));
    assert_eq!(row.get::<Decimal, _>("inventory_value"), d("10"));
    let sequences: Vec<i64> =
        sqlx::query_scalar("SELECT movement_seq FROM inventory_movements ORDER BY movement_seq")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(sequences, vec![1, 2, 3]);
    assert!(
        sqlx::query("UPDATE inventory_movements SET reference='tamper' WHERE id=$1")
            .bind(legacy_receipt)
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn insertion_sequence_blocks_later_stock_even_when_transaction_timestamp_is_older(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie).await;
    let product_id = Uuid::parse_str(id(&stock)).unwrap();
    let mut early_tx = pool.begin().await.unwrap();
    // now() is fixed at transaction start, before the receipt acquires its lock.
    let _: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&mut *early_tx)
        .await
        .unwrap();
    let original = receipt(&app, &cookie, &stock, "1", "0.01").await;
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *early_tx)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
        .bind(product_id)
        .fetch_one(&mut *early_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE products SET quantity=quantity-0.001 WHERE id=$1")
        .bind(product_id)
        .execute(&mut *early_tx)
        .await
        .unwrap();
    let later_id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,created_by) VALUES($1,$2,'adjustment',-0.001,0.01,0,$3)")
        .bind(later_id).bind(product_id).bind(admin).execute(&mut *early_tx).await.unwrap();
    early_tx.commit().await.unwrap();
    let actual_order:bool=sqlx::query_scalar("SELECT later.created_at<receipt.created_at AND later.movement_seq>receipt.movement_seq FROM inventory_movements later CROSS JOIN inventory_movements receipt WHERE later.id=$1 AND receipt.id=$2")
        .bind(later_id).bind(Uuid::parse_str(id(&original)).unwrap()).fetch_one(&pool).await.unwrap();
    assert!(actual_order);
    let response = request(
        &app,
        &cookie,
        "POST",
        &format!("/api/inventory/receipts/{}/reverse", id(&original)),
        Some(reverse_body()),
    )
    .await;
    assert_eq!(response.0, StatusCode::CONFLICT);
    let movements = call(&app, &cookie, "GET", "/api/inventory/movements", None).await;
    assert_eq!(movements["items"][0]["id"], json!(later_id));
    assert_eq!(movements["items"][1]["can_reverse"], false);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM posting_reversals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
