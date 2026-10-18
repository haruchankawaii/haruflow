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

async fn actor(pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Goods receipt tester',$2,'fixture-unused',$3)")
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
    sqlx::query("INSERT INTO company(id,name) VALUES($1,'Goods receipt test')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let (admin, cookie) = actor(pool, "admin").await;
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "purchase-receipt-fixture-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        ".local/purchase-receipts-no-static".into(),
    );
    (app, admin, cookie)
}

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap()
}
fn uid(value: &Value) -> Uuid {
    id(value).parse().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn amount(value: &Value, field: &str) -> Decimal {
    d(value[field].as_str().unwrap())
}
fn action(order: &Value) -> Value {
    json!({"version":order["version"],"idempotency_key":Uuid::new_v4()})
}
fn reverse_body(order: &Value) -> Value {
    json!({"version":order["version"],"reason":"Barang diterima dengan biaya yang keliru","idempotency_key":Uuid::new_v4()})
}
fn receive_path(order: &Value) -> String {
    format!("/api/purchase-orders/{}/receive", id(order))
}
fn reverse_path(receipt: &Value) -> String {
    format!("/api/purchase-receipts/{}/reverse", id(receipt))
}
fn active_receipt(order: &Value) -> &Value {
    order["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == order["active_receipt_id"])
        .unwrap()
}

async fn supplier(app: &Router, cookie: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/suppliers",
        Some(json!({"code":"GR-SUP","name":"Supplier Gudang","idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn product(app: &Router, cookie: &str, sku: &str) -> Value {
    call(app,cookie,"POST","/api/products",Some(json!({"sku":sku,"name":format!("Barang {sku}"),"kind":"stock","unit":"kg","price":"80"}))).await
}
async fn order(app: &Router, cookie: &str, supplier: &Value, items: Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/purchase-orders",
        Some(json!({"supplier_id":supplier["id"],"items":items,"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn confirm(app: &Router, cookie: &str, order: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-orders/{}/confirm", id(order)),
        Some(action(order)),
    )
    .await
}
async fn simple_confirmed(app: &Router, cookie: &str, supplier: &Value, product: &Value) -> Value {
    let draft = order(
        app,
        cookie,
        supplier,
        json!([{"product_id":product["id"],"quantity":"1.125","unit_cost":"12.3456"}]),
    )
    .await;
    confirm(app, cookie, &draft).await
}
async fn receive(app: &Router, cookie: &str, order: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        &receive_path(order),
        Some(action(order)),
    )
    .await
}
async fn manual_receipt(
    app: &Router,
    cookie: &str,
    product: &Value,
    quantity: &str,
    cost: &str,
) -> Value {
    call(app,cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":product["id"],"quantity":quantity,"unit_cost":cost,"idempotency_key":Uuid::new_v4()}))).await
}
async fn stock(pool: &PgPool, product: &Value) -> (Decimal, Decimal, Decimal) {
    sqlx::query_as("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
        .bind(uid(product))
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn posting_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('products',(SELECT jsonb_agg(jsonb_build_object('id',id,'quantity',quantity,'average_cost',average_cost,'value',inventory_value) ORDER BY id) FROM products),'orders',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM purchase_orders p),'journals',(SELECT count(*) FROM journal_entries),'movements',(SELECT count(*) FROM inventory_movements),'receipts',(SELECT count(*) FROM purchase_receipts),'reversals',(SELECT count(*) FROM posting_reversals),'audits',(SELECT count(*) FROM audit_events))")
        .fetch_one(pool).await.unwrap()
}
async fn assert_inventory_matches_ledger(pool: &PgPool) {
    let (inventory, ledger): (Decimal, Decimal) = sqlx::query_as("SELECT (SELECT COALESCE(sum(inventory_value),0) FROM products),(SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1300')")
        .fetch_one(pool).await.unwrap();
    assert_eq!(inventory, ledger);
    let unbalanced: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit)) u")
        .fetch_one(pool).await.unwrap();
    assert_eq!(unbalanced, 0);
}

async fn assert_receipt_append_guard(pool: &PgPool, app: &Router, cookie: &str, receipt: &Value) {
    let append = "INSERT INTO purchase_receipt_lines(id,receipt_id,position,movement_id,product_id,product_sku,product_name,unit,quantity,unit_cost,value,before_quantity,before_average_cost,before_inventory_value) SELECT $1,$2,position,movement_id,product_id,product_sku,product_name,unit,quantity,unit_cost,value,before_quantity,before_average_cost,before_inventory_value FROM purchase_receipt_lines WHERE receipt_id=$3 LIMIT 1";
    let error = sqlx::query(append)
        .bind(Uuid::new_v4())
        .bind(uid(receipt))
        .bind(uid(receipt))
        .execute(pool)
        .await
        .unwrap_err();
    assert!(
        error
            .as_database_error()
            .unwrap()
            .message()
            .contains("append"),
        "An append must fail at the posting guard, before unique or foreign key constraints: {error}"
    );

    // A logical restore preserves the source transaction as ordinary data. Its
    // value can coincide with a new transaction ID in the restored database.
    // The physical header insertion transaction must still prevent appending.
    let journal=call(app,cookie,"POST","/api/accounting/journals",Some(json!({"description":"Restore guard fixture","lines":[{"account_code":"1100","debit":"1","credit":"0"},{"account_code":"3000","debit":"0","credit":"1"}],"idempotency_key":Uuid::new_v4()}))).await;
    let mut target = pool.begin().await.unwrap();
    let transaction: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
        .fetch_one(&mut *target)
        .await
        .unwrap();
    let restored = Uuid::new_v4();
    sqlx::query("INSERT INTO purchase_receipts(id,number,order_id,reference,total,journal_id,created_by,posting_transaction) SELECT $1,'GR-RESTORE-GUARD',order_id,reference,total,$2,created_by,$3::text::xid8 FROM purchase_receipts WHERE id=$4")
        .bind(restored).bind(uid(&journal)).bind(transaction).bind(uid(receipt)).execute(pool).await.unwrap();
    let error = sqlx::query(append)
        .bind(Uuid::new_v4())
        .bind(restored)
        .bind(uid(receipt))
        .execute(&mut *target)
        .await
        .unwrap_err();
    assert!(
        error
            .as_database_error()
            .unwrap()
            .message()
            .contains("append"),
        "A restored header needs the physical insertion guard even when stored transaction IDs coincide: {error}"
    );
    target.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_posts_all_reviewed_lines_and_grni_without_cash_or_capital(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let first = product(&app, &cookie, "GR-ROUND-A").await;
    let second = product(&app, &cookie, "GR-ROUND-B").await;
    manual_receipt(&app, &cookie, &first, "2", "2.3456").await;
    let draft = order(
        &app,
        &cookie,
        &supplier,
        json!([
            {"product_id":first["id"],"quantity":"1.125","unit_cost":"12.3456"},
            {"product_id":second["id"],"quantity":"0.005","unit_cost":"1.0050"}
        ]),
    )
    .await;
    let confirmed = confirm(&app, &cookie, &draft).await;
    // A goods receipt must use the reviewed PO snapshots, not changed master data.
    call(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/products/{}", id(&first)),
        Some(
            json!({"sku":"GR-RENAMED","name":"Nama baru","kind":"stock","unit":"kg","price":"90"}),
        ),
    )
    .await;
    let mut body = action(&confirmed);
    body["reference"] = json!("  SJ-2026-001  ");
    let received = call(
        &app,
        &cookie,
        "POST",
        &receive_path(&confirmed),
        Some(body.clone()),
    )
    .await;
    assert_eq!(received["status"], "received");
    assert_eq!(received["version"], 3);
    assert!(received["received_at"].is_string());
    assert_eq!(received["items"], confirmed["items"]);
    let receipt = active_receipt(&received);
    assert_eq!(receipt["order_id"], confirmed["id"]);
    assert_eq!(receipt["number"], "GR-000001");
    assert_eq!(receipt["reference"], "SJ-2026-001");
    assert_eq!(receipt["can_reverse"], true);
    assert!(receipt["reversal"].is_null());
    assert_eq!(amount(receipt, "total"), d("13.90"));
    assert_eq!(receipt["items"].as_array().unwrap().len(), 2);
    for line in confirmed["items"].as_array().unwrap() {
        let received_line = receipt["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["product_id"] == line["product_id"])
            .unwrap();
        for field in [
            "product_sku",
            "product_name",
            "unit",
            "quantity",
            "unit_cost",
        ] {
            assert_eq!(
                received_line[field], line[field],
                "Receipt snapshot {field}"
            );
        }
        assert_eq!(amount(received_line, "value"), amount(line, "total"));
        let movement: (Uuid, Decimal, Decimal) =
            sqlx::query_as("SELECT product_id,quantity,value FROM inventory_movements WHERE id=$1")
                .bind(Uuid::parse_str(received_line["movement_id"].as_str().unwrap()).unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            movement.0,
            Uuid::parse_str(line["product_id"].as_str().unwrap()).unwrap()
        );
        assert_eq!(movement.1, amount(line, "quantity"));
        assert_eq!(movement.2, amount(line, "total"));
    }
    assert_eq!(
        stock(&pool, &first).await,
        (d("3.125"), d("5.9456"), d("18.58"))
    );
    assert_eq!(stock(&pool, &second).await, (d("0.005"), d("2"), d("0.01")));
    let lines:Vec<(String,Decimal,Decimal)>=sqlx::query_as("SELECT l.account_code,l.debit,l.credit FROM journal_lines l WHERE journal_id=$1 ORDER BY l.account_code")
        .bind(Uuid::parse_str(receipt["journal_id"].as_str().unwrap()).unwrap()).fetch_all(&pool).await.unwrap();
    let grni: String = sqlx::query_scalar(
        "SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        lines,
        vec![
            ("1300".into(), d("13.90"), Decimal::ZERO),
            (grni, Decimal::ZERO, d("13.90"))
        ]
    );
    let journal_path = format!(
        "/api/accounting/journals/{}",
        receipt["journal_id"].as_str().unwrap()
    );
    let journal = call(&app, &cookie, "GET", &journal_path, None).await;
    assert_eq!(journal["id"], receipt["journal_id"]);
    assert_eq!(journal["lines"].as_array().unwrap().len(), 2);
    assert_eq!(journal["can_reverse"], false);
    assert_eq!(
        request(&app, "", "GET", &journal_path, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "GET",
            &format!("/api/accounting/journals/{}", Uuid::new_v4()),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_inventory_matches_ledger(&pool).await;
    let snapshot = posting_snapshot(&pool).await;
    assert_eq!(
        call(&app, &cookie, "POST", &receive_path(&confirmed), Some(body)).await,
        received
    );
    assert_eq!(posting_snapshot(&pool).await, snapshot);
    assert_eq!(
        call(
            &app,
            &cookie,
            "GET",
            &format!("/api/purchase-orders/{}", id(&confirmed)),
            None
        )
        .await,
        received
    );
    assert!(
        sqlx::query("UPDATE purchase_receipts SET reference='tamper' WHERE id=$1")
            .bind(uid(receipt))
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM purchase_receipt_lines WHERE receipt_id=$1")
            .bind(uid(receipt))
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE inventory_movements SET value=value WHERE purchase_receipt_id=$1")
            .bind(uid(receipt))
            .execute(&pool)
            .await
            .is_err()
    );
    assert_receipt_append_guard(&pool, &app, &cookie, receipt).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_reversal_restores_exact_prior_average_and_retains_rereceipt_history(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-RESTORE").await;
    manual_receipt(&app, &cookie, &product, "3", "3.3333").await;
    call(&app,&cookie,"POST","/api/inventory/adjustments",Some(json!({"product_id":product["id"],"quantity":"-1","reason":"One damaged unit","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(
        stock(&pool, &product).await,
        (d("2"), d("3.3333"), d("6.67"))
    );
    let draft = order(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":product["id"],"quantity":"1","unit_cost":"5"}]),
    )
    .await;
    let confirmed = confirm(&app, &cookie, &draft).await;
    let received = receive(&app, &cookie, &confirmed).await;
    let original = active_receipt(&received).clone();
    let body = reverse_body(&received);
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &reverse_path(&original),
        Some(body.clone()),
    )
    .await;
    assert_eq!(reversed["status"], "confirmed");
    assert_eq!(reversed["version"], 4);
    assert!(reversed["active_receipt_id"].is_null());
    assert!(reversed["received_at"].is_null());
    let history = &reversed["receipts"][0];
    assert_eq!(history["id"], original["id"]);
    assert_eq!(history["can_reverse"], false);
    assert_eq!(history["reversal"]["reason"], body["reason"]);
    assert_eq!(
        stock(&pool, &product).await,
        (d("2"), d("3.3333"), d("6.67"))
    );
    assert_inventory_matches_ledger(&pool).await;
    let inverse:Vec<(String,Decimal,Decimal)>=sqlx::query_as("SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY account_code")
        .bind(Uuid::parse_str(history["reversal"]["journal_id"].as_str().unwrap()).unwrap()).fetch_all(&pool).await.unwrap();
    let source:Vec<(String,Decimal,Decimal)>=sqlx::query_as("SELECT account_code,credit,debit FROM journal_lines WHERE journal_id=$1 ORDER BY account_code")
        .bind(Uuid::parse_str(original["journal_id"].as_str().unwrap()).unwrap()).fetch_all(&pool).await.unwrap();
    assert_eq!(inverse, source);
    let snapshot = posting_snapshot(&pool).await;
    assert_eq!(
        call(&app, &cookie, "POST", &reverse_path(&original), Some(body)).await,
        reversed
    );
    assert_eq!(posting_snapshot(&pool).await, snapshot);
    let rereceived = receive(&app, &cookie, &reversed).await;
    assert_eq!(rereceived["version"], 5);
    assert_eq!(rereceived["receipts"].as_array().unwrap().len(), 2);
    assert_ne!(active_receipt(&rereceived)["id"], original["id"]);
    assert_ne!(active_receipt(&rereceived)["number"], original["number"]);
    assert_eq!(amount(active_receipt(&rereceived), "total"), d("5"));
    assert_eq!(
        stock(&pool, &product).await,
        (d("3"), d("3.89"), d("11.67"))
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_path(&original),
            Some(reverse_body(&rereceived))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_failures_leave_no_partial_stock_journal_or_document(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let a = product(&app, &cookie, "GR-ATOMIC-A").await;
    let b = product(&app, &cookie, "GR-ATOMIC-B").await;
    let (valid, full) = if uid(&a) < uid(&b) {
        (&a, &b)
    } else {
        (&b, &a)
    };
    manual_receipt(&app, &cookie, full, "1000000000", "1").await;
    let draft = order(
        &app,
        &cookie,
        &supplier,
        json!([
            {"product_id":valid["id"],"quantity":"1","unit_cost":"10"},
            {"product_id":full["id"],"quantity":"1","unit_cost":"10"}
        ]),
    )
    .await;
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &receive_path(&draft),
            Some(action(&draft))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    let confirmed = confirm(&app, &cookie, &draft).await;
    let before = posting_snapshot(&pool).await;
    let (status, value) = request(
        &app,
        &cookie,
        "POST",
        &receive_path(&confirmed),
        Some(action(&confirmed)),
    )
    .await;
    assert!(
        status.is_client_error(),
        "Overflow must be rejected atomically: {status}: {value}"
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    assert_eq!(
        stock(&pool, valid).await,
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
    );
    let mut stale = action(&confirmed);
    stale["version"] = json!(1);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &receive_path(&confirmed),
            Some(stale)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let simple = simple_confirmed(&app, &cookie, &supplier, valid).await;
    for extra in [
        json!({"quantity":"0.1"}),
        json!({"reference":"a".repeat(201)}),
        json!({"reference":"bad\u{0000}reference"}),
    ] {
        let mut invalid = action(&simple);
        invalid
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let before = posting_snapshot(&pool).await;
        let (status, value) = request(
            &app,
            &cookie,
            "POST",
            &receive_path(&simple),
            Some(invalid.clone()),
        )
        .await;
        assert!(
            status.is_client_error(),
            "Invalid receipt input {invalid} must return a client error: {status}: {value}"
        );
        assert_eq!(posting_snapshot(&pool).await, before);
    }
    // Confirmed quantities have a reviewed unit. Receiving after the catalog's
    // unit changes would silently interpret those quantities in another unit.
    let update_unit = |unit: &str| json!({"sku":valid["sku"],"name":valid["name"],"kind":"stock","unit":unit,"price":valid["price"]});
    call(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/products/{}", id(valid)),
        Some(update_unit("box")),
    )
    .await;
    let body = action(&simple);
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &receive_path(&simple),
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        posting_snapshot(&pool).await,
        before,
        "A unit mismatch must preserve all quantities, postings and PO state"
    );
    call(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/products/{}", id(valid)),
        Some(update_unit("kg")),
    )
    .await;
    let received = call(&app, &cookie, "POST", &receive_path(&simple), Some(body)).await;
    assert_eq!(
        received["status"], "received",
        "A rejected unit mismatch must not consume the request's idempotency key"
    );
    assert_eq!(
        stock(&pool, valid).await,
        (d("1.125"), d("12.3467"), d("13.89"))
    );
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_closed_period_blocks_receive_and_reverse_without_side_effects(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-PERIOD").await;
    let confirmed = simple_confirmed(&app, &cookie, &supplier, &product).await;
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &receive_path(&confirmed),
            Some(action(&confirmed))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    sqlx::query("UPDATE accounting_period SET closed_through=NULL WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    let received = receive(&app, &cookie, &confirmed).await;
    let expected = stock(&pool, &product).await;
    sqlx::query("UPDATE products SET average_cost=average_cost+0.0001 WHERE id=$1")
        .bind(uid(&product))
        .execute(&pool)
        .await
        .unwrap();
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_path(active_receipt(&received)),
            Some(reverse_body(&received))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        posting_snapshot(&pool).await,
        before,
        "A changed carrying snapshot must not be rewound even without a later stock movement"
    );
    sqlx::query("UPDATE products SET average_cost=$2 WHERE id=$1")
        .bind(uid(&product))
        .bind(expected.1)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_path(active_receipt(&received)),
            Some(reverse_body(&received))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    let detail = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/purchase-orders/{}", id(&received)),
        None,
    )
    .await;
    assert_eq!(active_receipt(&detail)["can_reverse"], false);
    assert!(active_receipt(&detail)["reversal_blocked_reason"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_later_movement_on_any_line_blocks_entire_reversal_even_if_reversed(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let a = product(&app, &cookie, "GR-HISTORY-A").await;
    let b = product(&app, &cookie, "GR-HISTORY-B").await;
    let draft = order(
        &app,
        &cookie,
        &supplier,
        json!([
            {"product_id":a["id"],"quantity":"2","unit_cost":"10"},
            {"product_id":b["id"],"quantity":"3","unit_cost":"20"}
        ]),
    )
    .await;
    let confirmed = confirm(&app, &cookie, &draft).await;
    let received = receive(&app, &cookie, &confirmed).await;
    let later = manual_receipt(&app, &cookie, &b, "1", "5").await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/inventory/receipts/{}/reverse", id(&later)),
        Some(json!({"reason":"Reverse later receipt","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(stock(&pool, &a).await, (d("2"), d("10"), d("20")));
    assert_eq!(stock(&pool, &b).await, (d("3"), d("20"), d("60")));
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_path(active_receipt(&received)),
            Some(reverse_body(&received))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    let detail = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/purchase-orders/{}", id(&received)),
        None,
    )
    .await;
    assert_eq!(active_receipt(&detail)["can_reverse"], false);
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_concurrent_retries_bind_actor_body_and_operation_once(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-RETRY").await;
    let confirmed = simple_confirmed(&app, &cookie, &supplier, &product).await;
    let path = receive_path(&confirmed);
    let body = action(&confirmed);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(body.clone())),
        request(&app, &cookie, "POST", &path, Some(body.clone()))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(a, b);
    let received = a.1;
    let (_, warehouse) = actor(&pool, "warehouse").await;
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(&app, &warehouse, "POST", &path, Some(body.clone()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut changed = body.clone();
    changed["reference"] = json!("Different delivery note");
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(changed)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(action(&confirmed)))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let reverse = reverse_path(active_receipt(&received));
    let mut cross_operation = reverse_body(&received);
    cross_operation["idempotency_key"] = body["idempotency_key"].clone();
    assert_eq!(
        request(&app, &cookie, "POST", &reverse, Some(cross_operation))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    for reason in [
        "ab".to_owned(),
        "a".repeat(1001),
        "Invalid\u{0000}reason".to_owned(),
    ] {
        let mut invalid = reverse_body(&received);
        invalid["reason"] = json!(reason);
        let (status, value) = request(&app, &cookie, "POST", &reverse, Some(invalid)).await;
        assert!(
            status.is_client_error(),
            "Invalid reversal reason must return a client error: {status}: {value}"
        );
        assert_eq!(posting_snapshot(&pool).await, before);
    }
    let mut stale = reverse_body(&received);
    stale["version"] = confirmed["version"].clone();
    assert_eq!(
        request(&app, &cookie, "POST", &reverse, Some(stale))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    let body = reverse_body(&received);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &reverse, Some(body.clone())),
        request(&app, &cookie, "POST", &reverse, Some(body.clone()))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(a, b);
    let reversed = a.1;
    assert_eq!(
        request(&app, &warehouse, "POST", &reverse, Some(body))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(action(&reversed))),
        request(&app, &cookie, "POST", &path, Some(action(&reversed)))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT),
        "{a:?}, {b:?}"
    );
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM purchase_receipts),(SELECT count(*) FROM inventory_movements),(SELECT count(*) FROM journal_entries)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (2, 3, 3));
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_permissions_and_inactive_supplier_allow_existing_confirmed_delivery(
    pool: PgPool,
) {
    let (app, _, admin) = fixture(&pool).await;
    let supplier = supplier(&app, &admin).await;
    let product = product(&app, &admin, "GR-ROLE").await;
    let confirmed = simple_confirmed(&app, &admin, &supplier, &product).await;
    call(&app,&admin,"PUT",&format!("/api/suppliers/{}",id(&supplier)),Some(json!({"code":supplier["code"],"name":supplier["name"],"active":false,"version":supplier["version"],"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(
        request(&app, "", "GET", "/api/purchasing/summary", None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "",
            "POST",
            &receive_path(&confirmed),
            Some(action(&confirmed))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let mut readers = Vec::new();
    for role in ["sales", "finance", "viewer"] {
        let (_, cookie) = actor(&pool, role).await;
        assert_eq!(
            request(&app, &cookie, "GET", "/api/purchasing/summary", None)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                &receive_path(&confirmed),
                Some(action(&confirmed))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        readers.push(cookie);
    }
    let (_, warehouse) = actor(&pool, "warehouse").await;
    let received = receive(&app, &warehouse, &confirmed).await;
    for cookie in readers {
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                &reverse_path(active_receipt(&received)),
                Some(reverse_body(&received))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let reversed = call(
        &app,
        &warehouse,
        "POST",
        &reverse_path(active_receipt(&received)),
        Some(reverse_body(&received)),
    )
    .await;
    assert_eq!(reversed["status"], "confirmed");
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_must_be_reversed_before_order_can_be_cancelled(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-CANCEL").await;
    let confirmed = simple_confirmed(&app, &cookie, &supplier, &product).await;
    let received = receive(&app, &cookie, &confirmed).await;
    let path = format!("/api/purchase-orders/{}/cancel", id(&received));
    let cancel = |order: &Value| json!({"version":order["version"],"reason":"PO dibatalkan setelah koreksi penerimaan","idempotency_key":Uuid::new_v4()});
    let before = posting_snapshot(&pool).await;
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(cancel(&received)))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(posting_snapshot(&pool).await, before);
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &reverse_path(active_receipt(&received)),
        Some(reverse_body(&received)),
    )
    .await;
    let cancelled = call(&app, &cookie, "POST", &path, Some(cancel(&reversed))).await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["receipts"].as_array().unwrap().len(), 1);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &receive_path(&cancelled),
            Some(action(&cancelled))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        stock(&pool, &product).await,
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
    );
    assert_inventory_matches_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchase_receipt_summary_covers_all_orders_beyond_history_limit_and_tracks_reversals(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-SUMMARY").await;
    let confirmed = simple_confirmed(&app, &cookie, &supplier, &product).await;
    let received = receive(&app, &cookie, &confirmed).await;
    sqlx::query("INSERT INTO purchase_orders(id,number,supplier_id,supplier_code,supplier_name,total,created_by) SELECT md5('summary-'||n)::uuid,'SUMMARY-'||n,$1,'GR-SUP','Supplier Gudang',1,$2 FROM generate_series(1,501) n")
        .bind(uid(&supplier)).bind(admin).execute(&pool).await.unwrap();
    let list = call(&app, &cookie, "GET", "/api/purchase-orders", None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 500);
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(
        summary["counts"],
        json!({"draft":501,"confirmed":0,"received":1,"cancelled":0})
    );
    assert_eq!(amount(&summary["values"], "draft"), d("501"));
    assert_eq!(amount(&summary["values"], "received"), d("13.89"));
    assert_eq!(amount(&summary, "pending_receipt_total"), Decimal::ZERO);
    assert_eq!(amount(&summary, "received_unbilled_total"), d("13.89"));
    assert!(summary["as_of"].is_string());
    call(
        &app,
        &cookie,
        "POST",
        &reverse_path(active_receipt(&received)),
        Some(reverse_body(&received)),
    )
    .await;
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(
        summary["counts"],
        json!({"draft":501,"confirmed":1,"received":0,"cancelled":0})
    );
    assert_eq!(amount(&summary, "pending_receipt_total"), d("13.89"));
    assert_eq!(amount(&summary, "received_unbilled_total"), Decimal::ZERO);
}

#[sqlx::test(migrations = false)]
async fn purchase_receipt_upgrade_preserves_custom_account_and_blocks_control_account_manual_posts(
    pool: PgPool,
) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|m| m.version <= 10)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES('2110','Utang khusus sebelum upgrade','liability')").execute(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let custom: (String, String, Option<String>) =
        sqlx::query_as("SELECT name,kind,system_role FROM accounts WHERE code='2110'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        custom,
        (
            "Utang khusus sebelum upgrade".into(),
            "liability".into(),
            None
        )
    );
    let grni: (String, String) = sqlx::query_as(
        "SELECT code,kind FROM accounts WHERE system_role='purchase_receipt_accrual'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(grni, ("2111".into(), "liability".into()));
    let (app, _, cookie) = fixture(&pool).await;
    let accounts = call(&app, &cookie, "GET", "/api/accounting/accounts", None).await;
    let control = accounts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["code"] == grni.0)
        .unwrap();
    assert_eq!(control["is_control"], true);
    let lines = json!([{"account_code":"1100","debit":"10","credit":"0"},{"account_code":grni.0,"debit":"0","credit":"10"}]);
    let before = posting_snapshot(&pool).await;
    assert!(request(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Attempt direct GRNI credit","lines":lines,"idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    assert_eq!(posting_snapshot(&pool).await, before);
    assert!(request(&app,&cookie,"PUT","/api/accounting/opening",Some(json!({"version":0,"description":"Attempt direct opening GRNI credit","items":[],"lines":lines}))).await.0.is_client_error());
    let supplier = supplier(&app, &cookie).await;
    let product = product(&app, &cookie, "GR-UPGRADE").await;
    let confirmed = simple_confirmed(&app, &cookie, &supplier, &product).await;
    let received = receive(&app, &cookie, &confirmed).await;
    let credit: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(credit-debit),0) FROM journal_lines WHERE account_code='2111'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(credit, amount(active_receipt(&received), "total"));
}
