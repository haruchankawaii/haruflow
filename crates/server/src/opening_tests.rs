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

use crate::{AppState, app_router};

const OPENING: &str = "/api/accounting/opening";

async fn request(
    app: &Router,
    cookie: Option<&str>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-haruflow-request", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, body)
}

async fn call(app: &Router, cookie: &str, method: &str, path: &str, body: Option<Value>) -> Value {
    let (status, value) = request(app, Some(cookie), method, path, body).await;
    assert!(status.is_success(), "{method} {path}: {status}: {value}");
    value
}

async fn user(pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role,active) VALUES($1,'Opening tester',$2,'fixture-unused-hash',$3,true)")
        .bind(id).bind(format!("opening-{id}@example.test")).bind(role).execute(pool).await.unwrap();
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
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Opening Community Test')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let (admin_id, cookie) = user(pool, "admin").await;
    let app = app_router(
        AppState {
            db: pool.clone(),
            setup_token: "opening-test-setup-token-over-32-characters".into(),
            secure_cookies: false,
        },
        ".local/opening-tests-no-static".into(),
    );
    (app, admin_id, cookie)
}

async fn product(app: &Router, cookie: &str, kind: &str) -> Value {
    call(app, cookie, "POST", "/api/products", Some(json!({"sku":format!("OPEN-{}",Uuid::new_v4().simple()),"name":"Opening product","kind":kind,"price":"80000"}))).await
}

fn draft(version: i64, stock: Option<&Value>) -> Value {
    json!({"version":version,"description":"Opening balances from the previous books",
        "items":stock.map(|p|vec![json!({"product_id":p["id"],"quantity":"10","unit_cost":"50000"})]).unwrap_or_default(),
        "lines":[{"account_code":"1100","debit":"100000","credit":"0"},
            {"account_code":"3000","debit":"0","credit":if stock.is_some(){"600000"}else{"100000"}}]})
}

fn version(envelope: &Value) -> i64 {
    envelope["document"]["version"].as_i64().unwrap()
}
fn amount(value: &Value, field: &str) -> Decimal {
    value[field].as_str().unwrap().parse().unwrap()
}
fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}

async fn post(app: &Router, cookie: &str, version: i64) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("{OPENING}/post"),
        Some(json!({"version":version,"idempotency_key":Uuid::new_v4()})),
    )
    .await
}

async fn invoice_service(app: &Router, cookie: &str) {
    let service = product(app, cookie, "service").await;
    let customer = call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Opening customer"})),
    )
    .await;
    let order = call(app, cookie, "POST", "/api/orders", Some(json!({"customer_id":customer["id"],"items":[{"product_id":service["id"],"quantity":"1","unit_price":"80000"}],"idempotency_key":Uuid::new_v4()}))).await;
    for action in ["confirm", "invoice"] {
        call(
            app,
            cookie,
            "POST",
            &format!("/api/orders/{}/{action}", order["id"].as_str().unwrap()),
            Some(json!({"idempotency_key":Uuid::new_v4()})),
        )
        .await;
    }
}

async fn counts(pool: &PgPool) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM journal_entries),(SELECT count(*) FROM inventory_movements)",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn assert_stock_and_gl(pool: &PgPool, id: Uuid, quantity: i64, cost: i64, value: i64) {
    let stock: (Decimal, Decimal, Decimal) =
        sqlx::query_as("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        stock,
        (
            Decimal::from(quantity),
            Decimal::from(cost),
            Decimal::from(value)
        )
    );
    let gl: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1300'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        gl,
        Decimal::from(value),
        "stock valuation must agree with inventory control account"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_posts_cash_and_stock_once_with_balanced_inventory_control(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let before = call(&app, &cookie, "GET", OPENING, None).await;
    assert_eq!(before["eligible"], true);
    assert!(before["document"].is_null());
    let stock = product(&app, &cookie, "stock").await;
    let saved = call(&app, &cookie, "PUT", OPENING, Some(draft(0, Some(&stock)))).await;
    assert_eq!(version(&saved), 1);
    assert_eq!(saved["document"]["status"], "draft");
    assert_eq!(
        amount(&saved["document"], "inventory_total"),
        Decimal::from(500000)
    );
    assert_eq!(
        amount(&saved["document"], "total_debit"),
        Decimal::from(600000)
    );
    assert_eq!(saved["document"]["balanced"], true);
    assert_eq!(saved["document"]["lines"].as_array().unwrap().len(), 3);
    assert_eq!(
        counts(&pool).await,
        (0, 0),
        "draft previews cannot post anything"
    );
    let body = json!({"version":1,"idempotency_key":Uuid::new_v4()});
    let posted = call(
        &app,
        &cookie,
        "POST",
        &format!("{OPENING}/post"),
        Some(body.clone()),
    )
    .await;
    assert_eq!(posted["document"]["status"], "posted");
    assert_eq!(version(&posted), 2);
    assert_eq!(posted["eligible"], false);
    assert_eq!(posted["can_reverse"], true);
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &format!("{OPENING}/post"),
            Some(body.clone())
        )
        .await,
        posted
    );
    let mut changed_retry = body;
    changed_retry["version"] = json!(2);
    let (status, _) = request(
        &app,
        Some(&cookie),
        "POST",
        &format!("{OPENING}/post"),
        Some(changed_retry),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "idempotency key cannot be reused with another request"
    );
    let (status, _) = request(
        &app,
        Some(&cookie),
        "POST",
        &format!("{OPENING}/post"),
        Some(json!({"version":2,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(counts(&pool).await, (1, 1));
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 10, 50000, 500000).await;
    let movement: (String, Decimal, Decimal) =
        sqlx::query_as("SELECT kind,quantity,value FROM inventory_movements")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        movement,
        ("opening".into(), Decimal::from(10), Decimal::from(500000))
    );
    let trial = call(&app, &cookie, "GET", "/api/accounting/trial-balance", None).await;
    assert_eq!(trial["balanced"], true);
    assert_eq!(amount(&trial, "total_debit"), Decimal::from(600000));
    assert_eq!(amount(&trial, "total_credit"), Decimal::from(600000));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM journal_lines WHERE account_code='1200'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    invoice_service(&app, &cookie).await;
    let blocked = call(&app, &cookie, "GET", OPENING, None).await;
    assert_eq!(blocked["can_reverse"], false);
    let (status, _) = request(&app, Some(&cookie), "POST", &format!("{OPENING}/reverse"), Some(json!({"version":2,"reason":"Too late after a service invoice","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"],
        posted["document"]
    );
    assert_eq!(counts(&pool).await, (2, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_opening_posts_with_different_keys_have_only_one_effect(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie, "stock").await;
    let mut boundary = draft(0, Some(&stock));
    let mut lines = Vec::new();
    for index in 0..100 {
        let code = format!("8{index:03}");
        let is_debit = index < 50;
        sqlx::query(
            "INSERT INTO accounts(code,name,kind) VALUES($1,'Boundary opening account',$2)",
        )
        .bind(&code)
        .bind(if is_debit { "asset" } else { "equity" })
        .execute(&pool)
        .await
        .unwrap();
        lines.push(json!({"account_code":code,"debit":if is_debit{"10000"}else{"0"},"credit":if is_debit{"0"}else{"20000"}}));
    }
    boundary["lines"] = json!(lines);
    let saved = call(&app, &cookie, "PUT", OPENING, Some(boundary)).await;
    assert_eq!(
        saved["document"]["lines"].as_array().unwrap().len(),
        101,
        "100 manual lines plus the generated inventory line are permitted"
    );
    let path = format!("{OPENING}/post");
    let (first, second) = tokio::join!(
        request(
            &app,
            Some(&cookie),
            "POST",
            &path,
            Some(json!({"version":1,"idempotency_key":Uuid::new_v4()}))
        ),
        request(
            &app,
            Some(&cookie),
            "POST",
            &path,
            Some(json!({"version":1,"idempotency_key":Uuid::new_v4()}))
        )
    );
    assert_eq!(
        [first.0, second.0]
            .iter()
            .filter(|s| s.is_success())
            .count(),
        1,
        "{first:?} {second:?}"
    );
    assert_eq!(
        [first.0, second.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    assert_eq!(counts(&pool).await, (1, 1));
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 10, 50000, 500000).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_lines")
            .fetch_one(&pool)
            .await
            .unwrap(),
        101
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_drafts_validate_references_roles_and_versions_without_partial_posting(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie, "stock").await;
    let service = product(&app, &cookie, "service").await;
    let (_, finance) = user(&pool, "finance").await;
    let (status, _) = request(&app, None, "GET", OPENING, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for role in ["viewer", "sales", "warehouse"] {
        let (_, role_cookie) = user(&pool, role).await;
        let (status, _) = request(&app, Some(&role_cookie), "GET", OPENING, None).await;
        assert_eq!(status, StatusCode::OK);
        for (method, path, body) in [
            ("PUT", OPENING.to_owned(), draft(0, Some(&stock))),
            (
                "POST",
                format!("{OPENING}/post"),
                json!({"version":0,"idempotency_key":Uuid::new_v4()}),
            ),
            (
                "POST",
                format!("{OPENING}/reverse"),
                json!({"version":0,"reason":"Correction requested","idempotency_key":Uuid::new_v4()}),
            ),
        ] {
            let (status, _) = request(&app, Some(&role_cookie), method, &path, Some(body)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{role}: {method} {path}");
        }
    }
    let mut invalids = Vec::new();
    for code in ["1200", "1300", "4000", "5000", "MISSING"] {
        let mut value = draft(0, Some(&stock));
        value["lines"][0]["account_code"] = json!(code);
        invalids.push(value);
    }
    for product_id in [service["id"].clone(), json!(Uuid::new_v4())] {
        let mut value = draft(0, Some(&stock));
        value["items"][0]["product_id"] = product_id;
        invalids.push(value);
    }
    for (field, amount) in [
        ("quantity", "0"),
        ("quantity", "-1"),
        ("quantity", "0.0001"),
        ("unit_cost", "-1"),
        ("unit_cost", "1.00001"),
        ("unit_cost", "0"),
    ] {
        let mut value = draft(0, Some(&stock));
        value["items"][0][field] = json!(amount);
        invalids.push(value);
    }
    let mut duplicate_product = draft(0, Some(&stock));
    duplicate_product["items"] =
        json!([duplicate_product["items"][0], duplicate_product["items"][0]]);
    invalids.push(duplicate_product);
    let mut duplicate_account = draft(0, None);
    duplicate_account["lines"] =
        json!([duplicate_account["lines"][0], duplicate_account["lines"][0]]);
    invalids.push(duplicate_account);
    for (field, amount) in [("debit", "1.001"), ("debit", "-1"), ("credit", "1")] {
        let mut value = draft(0, None);
        value["lines"][0][field] = json!(amount);
        invalids.push(value);
    }
    for value in invalids {
        let (status, error) =
            request(&app, Some(&finance), "PUT", OPENING, Some(value.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}: {error}");
    }
    assert_eq!(counts(&pool).await, (0, 0));
    let empty =
        json!({"version":0,"description":"Balances still being collected","items":[],"lines":[]});
    let saved = call(&app, &finance, "PUT", OPENING, Some(empty)).await;
    assert_eq!(version(&saved), 1);
    let (status, _) = request(
        &app,
        Some(&finance),
        "POST",
        &format!("{OPENING}/post"),
        Some(json!({"version":1,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut unbalanced = draft(1, Some(&stock));
    unbalanced["lines"][1]["credit"] = json!("1");
    let saved = call(&app, &finance, "PUT", OPENING, Some(unbalanced)).await;
    assert_eq!(version(&saved), 2);
    assert_eq!(saved["document"]["balanced"], false);
    let (status, _) = request(
        &app,
        Some(&finance),
        "POST",
        &format!("{OPENING}/post"),
        Some(json!({"version":2,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(
        &app,
        Some(&finance),
        "PUT",
        OPENING,
        Some(draft(1, Some(&stock))),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"],
        saved["document"]
    );
    assert_eq!(counts(&pool).await, (0, 0));
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 0, 0, 0).await;
    let mut precise = draft(2, Some(&stock));
    precise["items"][0]["quantity"] = json!("0.003");
    precise["items"][0]["unit_cost"] = json!("1.6667");
    precise["lines"][1]["credit"] = json!("100000.01");
    let corrected = call(&app, &finance, "PUT", OPENING, Some(precise)).await;
    assert_eq!(version(&corrected), 3);
    assert_eq!(
        amount(&corrected["document"], "inventory_total"),
        Decimal::new(1, 2),
        "quantity times unit cost is rounded once to money precision"
    );
    assert_eq!(
        post(&app, &finance, 3).await["document"]["status"],
        "posted"
    );
    let inventory: (Decimal, Decimal, Decimal) =
        sqlx::query_as("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
            .bind(uuid(&stock["id"]))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        inventory,
        (
            Decimal::new(3, 3),
            Decimal::new(33333, 4),
            Decimal::new(1, 2)
        )
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_reversal_retains_immutable_history_and_can_repost_corrected_balances(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie, "stock").await;
    call(&app, &cookie, "PUT", OPENING, Some(draft(0, Some(&stock)))).await;
    let posted = post(&app, &cookie, 1).await;
    let original = posted["document"]["history"][0].clone();
    let reversal = json!({"version":2,"reason":"Correct the stock count and opening cash","idempotency_key":Uuid::new_v4()});
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &format!("{OPENING}/reverse"),
        Some(reversal.clone()),
    )
    .await;
    assert_eq!(version(&reversed), 3);
    assert_eq!(reversed["document"]["status"], "draft");
    assert_eq!(reversed["eligible"], true);
    assert_eq!(reversed["can_reverse"], false);
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &format!("{OPENING}/reverse"),
            Some(reversal)
        )
        .await,
        reversed
    );
    let history = &reversed["document"]["history"][0];
    assert_eq!(history["id"], original["id"]);
    assert_eq!(history["items"], original["items"]);
    assert_eq!(history["lines"], original["lines"]);
    assert_eq!(history["description"], original["description"]);
    assert_eq!(
        history["reason"],
        "Correct the stock count and opening cash"
    );
    assert!(!history["reversed_at"].is_null());
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 0, 0, 0).await;
    assert_eq!(counts(&pool).await, (2, 2));
    let immutable_history =
        sqlx::query("UPDATE opening_postings SET description='Tampered snapshot' WHERE id=$1")
            .bind(uuid(&original["id"]))
            .execute(&pool)
            .await;
    assert!(
        immutable_history.is_err(),
        "opening snapshots cannot be edited"
    );
    let immutable_ledger =
        sqlx::query("UPDATE journal_entries SET description='Tampered' WHERE id=$1")
            .bind(uuid(&original["journal_id"]))
            .execute(&pool)
            .await;
    assert!(
        immutable_ledger.is_err(),
        "reversal must not edit the original journal"
    );
    let mut corrected = draft(3, Some(&stock));
    corrected["description"] = json!("Corrected opening balances");
    corrected["items"][0]["quantity"] = json!("4");
    corrected["items"][0]["unit_cost"] = json!("25000");
    corrected["lines"][0]["debit"] = json!("200000");
    corrected["lines"][1]["credit"] = json!("300000");
    let saved = call(&app, &cookie, "PUT", OPENING, Some(corrected)).await;
    assert_eq!(version(&saved), 4);
    let reposted = post(&app, &cookie, 4).await;
    assert_eq!(version(&reposted), 5);
    assert_eq!(reposted["document"]["history"].as_array().unwrap().len(), 2);
    let old = reposted["document"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == original["id"])
        .unwrap();
    assert_eq!(old, history);
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 4, 25000, 100000).await;
    let trial = call(&app, &cookie, "GET", "/api/accounting/trial-balance", None).await;
    assert_eq!(amount(&trial, "total_debit"), Decimal::from(300000));
    assert_eq!(amount(&trial, "total_credit"), Decimal::from(300000));
    assert_eq!(counts(&pool).await, (3, 3));
    let today: String = sqlx::query_scalar(
        "SELECT (now() AT TIME ZONE (SELECT timezone FROM company))::date::text",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    call(&app, &cookie, "POST", "/api/accounting/period/close", Some(json!({"through":today,"reason":"Close after corrected opening balances","idempotency_key":Uuid::new_v4()}))).await;
    let (status, _) = request(&app, Some(&cookie), "POST", &format!("{OPENING}/reverse"), Some(json!({"version":5,"reason":"Correction after the date was closed","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a posted opening cannot be reversed into a closed period"
    );
    assert_eq!(counts(&pool).await, (3, 3));
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 4, 25000, 100000).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_stock_receipt_blocks_opening_without_changing_existing_balances(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie, "stock").await;
    call(&app, &cookie, "PUT", OPENING, Some(draft(0, Some(&stock)))).await;
    call(&app, &cookie, "POST", "/api/inventory/receipts", Some(json!({"product_id":stock["id"],"quantity":"2","unit_cost":"75000","idempotency_key":Uuid::new_v4()}))).await;
    let blocked = call(&app, &cookie, "GET", OPENING, None).await;
    assert_eq!(blocked["eligible"], false);
    assert!(blocked["blocked_reason"].is_string());
    let (status, _) = request(
        &app,
        Some(&cookie),
        "POST",
        &format!("{OPENING}/post"),
        Some(json!({"version":1,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(counts(&pool).await, (1, 1));
    assert_stock_and_gl(&pool, uuid(&stock["id"]), 2, 75000, 150000).await;
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"]["status"],
        "draft"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn zero_value_business_movements_block_reversal_even_after_stock_is_restored(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let stock = product(&app, &cookie, "stock").await;
    let mut initial = draft(0, Some(&stock));
    initial["items"][0]["quantity"] = json!("1000");
    initial["items"][0]["unit_cost"] = json!("0.0001");
    initial["lines"][1]["credit"] = json!("100000.10");
    call(&app, &cookie, "PUT", OPENING, Some(initial)).await;
    let posted = post(&app, &cookie, 1).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Opening customer"})),
    )
    .await;
    let order = call(&app, &cookie, "POST", "/api/orders", Some(json!({"customer_id":customer["id"],"items":[{"product_id":stock["id"],"quantity":"0.001","unit_price":"80000"}],"idempotency_key":Uuid::new_v4()}))).await;
    for action in ["confirm", "ship", "reverse-shipment"] {
        call(
            &app,
            &cookie,
            "POST",
            &format!("/api/orders/{}/{action}", order["id"].as_str().unwrap()),
            Some(json!({"reason":"Restore the zero-value shipment","idempotency_key":Uuid::new_v4()})),
        )
        .await;
    }
    let restored: (Decimal, Decimal, Decimal) =
        sqlx::query_as("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
            .bind(uuid(&stock["id"]))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        restored,
        (Decimal::from(1000), Decimal::new(1, 4), Decimal::new(1, 1))
    );
    assert_eq!(
        counts(&pool).await,
        (1, 3),
        "zero-value stock postings do not create any additional journal"
    );
    let zero_movements: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_movements WHERE kind IN ('shipment','shipment_reversal') AND value=0").fetch_one(&pool).await.unwrap();
    assert_eq!(zero_movements, 2);
    let blocked = call(&app, &cookie, "GET", OPENING, None).await;
    assert_eq!(blocked["can_reverse"], false);
    let (status, _) = request(&app, Some(&cookie), "POST", &format!("{OPENING}/reverse"), Some(json!({"version":2,"reason":"Too late to reverse opening","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"],
        posted["document"]
    );
    assert_eq!(counts(&pool).await, (1, 3));
    let cash: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1100'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cash, Decimal::from(100000));
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_serializes_against_the_first_business_posting(pool: PgPool) {
    let (app, admin_id, cookie) = fixture(&pool).await;
    call(&app, &cookie, "PUT", OPENING, Some(draft(0, None))).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let app_clone = app.clone();
    let cookie_clone = cookie.clone();
    let mut posting = tokio::spawn(async move {
        request(
            &app_clone,
            Some(&cookie_clone),
            "POST",
            &format!("{OPENING}/post"),
            Some(json!({"version":1,"idempotency_key":Uuid::new_v4()})),
        )
        .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut posting)
            .await
            .is_err(),
        "opening must wait for the shared accounting lock"
    );
    let journal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Competing first business posting','manual',$1,$2)")
        .bind(journal_id).bind(admin_id).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1100',25,0),($3,$2,'3000',0,25)")
        .bind(Uuid::new_v4()).bind(journal_id).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (status, _) = tokio::time::timeout(std::time::Duration::from_secs(5), posting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(counts(&pool).await, (1, 0));
}
