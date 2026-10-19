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

const BILLS: &str = "/api/purchase-bills";

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
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)})),
    )
}
async fn call(app: &Router, cookie: &str, method: &str, path: &str, body: Option<Value>) -> Value {
    let (status, value) = request(app, cookie, method, path, body).await;
    assert!(status.is_success(), "{method} {path}: {status}: {value}");
    value
}
async fn actor(pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Supplier bill tester',$2,'unused-fixture',$3)")
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
    sqlx::query(
        "INSERT INTO company(id,name,timezone) VALUES($1,'Supplier bill test','Asia/Jakarta')",
    )
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
    let (admin, cookie) = actor(pool, "admin").await;
    (
        crate::app_router(
            crate::AppState {
                db: pool.clone(),
                setup_token: "supplier-bill-fixture-token-at-least-32-characters".into(),
                secure_cookies: false,
            },
            ".local/supplier-bills-no-static".into(),
        ),
        admin,
        cookie,
    )
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap()
}
fn uid(v: &Value) -> Uuid {
    id(v).parse().unwrap()
}
fn d(v: &str) -> Decimal {
    v.parse().unwrap()
}
fn amount(v: &Value, field: &str) -> Decimal {
    d(v[field].as_str().unwrap())
}
fn action(order: &Value) -> Value {
    json!({"version":order["version"],"idempotency_key":Uuid::new_v4()})
}
fn receipt(order: &Value) -> &Value {
    order["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == order["active_receipt_id"])
        .unwrap()
}
fn reverse_bill_path(bill: &Value) -> String {
    format!("{BILLS}/{}/reverse", id(bill))
}
fn reverse_bill(bill: &Value) -> Value {
    json!({"version":bill["version"],"reason":"Nomor invoice supplier perlu dikoreksi","idempotency_key":Uuid::new_v4()})
}
fn reverse_receipt_path(order: &Value) -> String {
    format!("/api/purchase-receipts/{}/reverse", id(receipt(order)))
}
fn reverse_receipt(order: &Value) -> Value {
    json!({"version":order["version"],"reason":"Penerimaan barang harus dikoreksi","idempotency_key":Uuid::new_v4()})
}
async fn supplier(app: &Router, cookie: &str, code: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/suppliers",
        Some(
            json!({"code":code,"name":format!("Supplier {code}"),"idempotency_key":Uuid::new_v4()}),
        ),
    )
    .await
}
async fn product(app: &Router, cookie: &str, sku: &str) -> Value {
    call(app,cookie,"POST","/api/products",Some(json!({"sku":sku,"name":format!("Barang {sku}"),"kind":"stock","unit":"kg","price":"80"}))).await
}
async fn received(app: &Router, cookie: &str, supplier: &Value, product: &Value) -> Value {
    let draft=call(app,cookie,"POST","/api/purchase-orders",Some(json!({"supplier_id":supplier["id"],"items":[{"product_id":product["id"],"quantity":"1.125","unit_cost":"12.3456"}],"idempotency_key":Uuid::new_v4()}))).await;
    let confirmed = call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-orders/{}/confirm", id(&draft)),
        Some(action(&draft)),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-orders/{}/receive", id(&confirmed)),
        Some(action(&confirmed)),
    )
    .await
}
async fn get_order(app: &Router, cookie: &str, order: &Value) -> Value {
    call(
        app,
        cookie,
        "GET",
        &format!("/api/purchase-orders/{}", id(order)),
        None,
    )
    .await
}
async fn bill_body(pool: &PgPool, order: &Value, number: &str) -> Value {
    let today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date::text FROM company")
            .fetch_one(pool)
            .await
            .unwrap();
    json!({"receipt_id":receipt(order)["id"],"order_version":order["version"],"supplier_invoice_number":number,"invoice_date":"2020-01-01","due_date":today,"notes":"Catatan tagihan asli","idempotency_key":Uuid::new_v4()})
}
async fn bill(pool: &PgPool, app: &Router, cookie: &str, order: &Value, number: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        BILLS,
        Some(bill_body(pool, order, number).await),
    )
    .await
}
async fn snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('products',(SELECT jsonb_agg(jsonb_build_object('id',id,'quantity',quantity,'average_cost',average_cost,'value',inventory_value) ORDER BY id) FROM products),'orders',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM purchase_orders p),'bills',(SELECT jsonb_agg(to_jsonb(b) ORDER BY id) FROM purchase_bills b),'journals',(SELECT count(*) FROM journal_entries),'movements',(SELECT count(*) FROM inventory_movements),'reversals',(SELECT count(*) FROM posting_reversals),'audits',(SELECT count(*) FROM audit_events))").fetch_one(pool).await.unwrap()
}
async fn stock_and_operating_balances(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('products',(SELECT jsonb_agg(jsonb_build_object('id',id,'quantity',quantity,'average_cost',average_cost,'value',inventory_value) ORDER BY id) FROM products),'movements',(SELECT count(*) FROM inventory_movements),'balances',(SELECT jsonb_object_agg(code,balance) FROM (SELECT a.code,COALESCE(sum(l.debit-l.credit),0) balance FROM accounts a LEFT JOIN journal_lines l ON l.account_code=a.code WHERE a.code IN ('1100','1300','4000','5000') GROUP BY a.code) s))").fetch_one(pool).await.unwrap()
}
async fn journal_lines(pool: &PgPool, journal: &Value) -> Vec<(String, Decimal, Decimal)> {
    sqlx::query_as("SELECT account_code,debit,credit FROM journal_lines WHERE journal_id=$1 ORDER BY account_code")
        .bind(Uuid::parse_str(journal.as_str().unwrap()).unwrap()).fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_post_void_and_reissue_preserve_stock_and_exact_receipt_snapshots(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-POST").await;
    let product = product(&app, &cookie, "AP-POST").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let before = stock_and_operating_balances(&pool).await;
    let body = bill_body(&pool, &received, "  INV-EXT-001  ").await;
    let posted = call(&app, &cookie, "POST", BILLS, Some(body.clone())).await;
    assert_eq!(posted["number"], "PB-000001");
    assert_eq!(posted["supplier_invoice_number"], "INV-EXT-001");
    assert_eq!(posted["supplier_id"], supplier["id"]);
    assert_eq!(posted["supplier_code"], supplier["code"]);
    assert_eq!(posted["supplier_name"], supplier["name"]);
    assert_eq!(posted["receipt_id"], receipt(&received)["id"]);
    assert_eq!(posted["receipt_number"], receipt(&received)["number"]);
    assert_eq!(posted["order_id"], received["id"]);
    assert_eq!(posted["order_number"], received["number"]);
    assert_eq!(posted["status"], "unpaid");
    assert_eq!(posted["due_status"], "due_today");
    assert_eq!(posted["version"], 1);
    assert_eq!(posted["can_reverse"], true);
    assert!(posted["reversal"].is_null());
    assert_eq!(amount(&posted, "paid"), Decimal::ZERO);
    assert_eq!(amount(&posted, "total"), d("13.89"));
    assert_eq!(amount(&posted, "outstanding"), d("13.89"));
    for line in posted["items"].as_array().unwrap() {
        let source = receipt(&received)["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["product_id"] == line["product_id"])
            .unwrap();
        for field in [
            "product_id",
            "product_sku",
            "product_name",
            "unit",
            "quantity",
            "unit_cost",
            "value",
        ] {
            assert_eq!(line[field], source[field], "Snapshot field {field}");
        }
    }
    let grni: String = sqlx::query_scalar(
        "SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ap: String =
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut expected = vec![
        (grni.clone(), d("13.89"), Decimal::ZERO),
        (ap.clone(), Decimal::ZERO, d("13.89")),
    ];
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(journal_lines(&pool, &posted["journal_id"]).await, expected);
    assert_eq!(stock_and_operating_balances(&pool).await, before);
    let after = snapshot(&pool).await;
    assert_eq!(call(&app, &cookie, "POST", BILLS, Some(body)).await, posted);
    assert_eq!(snapshot(&pool).await, after);
    assert_eq!(
        call(
            &app,
            &cookie,
            "GET",
            &format!("{BILLS}/{}", id(&posted)),
            None
        )
        .await,
        posted
    );
    let updated = get_order(&app, &cookie, &received).await;
    assert_eq!(
        updated["version"].as_i64().unwrap(),
        received["version"].as_i64().unwrap() + 1
    );
    assert_eq!(receipt(&updated)["can_bill"], false);
    assert_eq!(receipt(&updated)["bill_id"], posted["id"]);
    assert_eq!(receipt(&updated)["can_reverse"], false);
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(amount(&summary, "received_unbilled_total"), Decimal::ZERO);
    assert_eq!(amount(&summary, "supplier_payable_total"), d("13.89"));
    let reason = reverse_bill(&posted);
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &reverse_bill_path(&posted),
        Some(reason.clone()),
    )
    .await;
    assert_eq!(reversed["status"], "void");
    assert_eq!(reversed["due_status"], "void");
    assert_eq!(reversed["version"], 2);
    assert_eq!(reversed["reversal"]["reason"], reason["reason"]);
    assert_eq!(amount(&reversed, "outstanding"), Decimal::ZERO);
    for field in [
        "number",
        "supplier_id",
        "supplier_code",
        "supplier_name",
        "supplier_invoice_number",
        "receipt_id",
        "order_id",
        "total",
        "items",
        "invoice_date",
        "due_date",
        "notes",
        "journal_id",
    ] {
        assert_eq!(
            reversed[field], posted[field],
            "Immutable bill field {field}"
        );
    }
    let mut inverse = expected
        .into_iter()
        .map(|(a, debit, credit)| (a, credit, debit))
        .collect::<Vec<_>>();
    inverse.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        journal_lines(&pool, &reversed["reversal"]["journal_id"]).await,
        inverse
    );
    let after = snapshot(&pool).await;
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &reverse_bill_path(&posted),
            Some(reason)
        )
        .await,
        reversed
    );
    assert_eq!(snapshot(&pool).await, after);
    assert_eq!(stock_and_operating_balances(&pool).await, before);
    let updated = get_order(&app, &cookie, &received).await;
    assert_eq!(receipt(&updated)["can_bill"], true);
    assert_eq!(receipt(&updated)["can_reverse"], true);
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(amount(&summary, "received_unbilled_total"), d("13.89"));
    assert_eq!(amount(&summary, "supplier_payable_total"), Decimal::ZERO);
    let reissued = bill(&pool, &app, &cookie, &updated, "inv-ext-001").await;
    assert_ne!(posted["id"], reissued["id"]);
    assert_eq!(reissued["number"], "PB-000002");
    assert_eq!(stock_and_operating_balances(&pool).await, before);
    let unbalanced:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit)) x").fetch_one(&pool).await.unwrap();
    assert_eq!(unbalanced, 0);
    assert!(
        sqlx::query("UPDATE purchase_bills SET total=total+1 WHERE id=$1")
            .bind(uid(&posted))
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM purchase_bills WHERE id=$1")
            .bind(uid(&posted))
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE purchase_receipt_lines SET unit_cost=unit_cost+1 WHERE receipt_id=$1")
            .bind(uid(receipt(&received)))
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_external_number_uniqueness_is_trimmed_case_insensitive_and_supplier_scoped(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let first = supplier(&app, &cookie, "AP-FIRST").await;
    let second = supplier(&app, &cookie, "AP-SECOND").await;
    let product = product(&app, &cookie, "AP-DUP").await;
    let one = received(&app, &cookie, &first, &product).await;
    let posted = bill(&pool, &app, &cookie, &one, "  Ext-Aa-001  ").await;
    let two = received(&app, &cookie, &first, &product).await;
    let before = snapshot(&pool).await;
    let duplicate = bill_body(&pool, &two, "ext-AA-001").await;
    assert_eq!(
        request(&app, &cookie, "POST", BILLS, Some(duplicate))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
    let other = received(&app, &cookie, &second, &product).await;
    let allowed = bill(&pool, &app, &cookie, &other, "ext-AA-001").await;
    assert_eq!(allowed["supplier_id"], second["id"]);
    call(
        &app,
        &cookie,
        "POST",
        &reverse_bill_path(&posted),
        Some(reverse_bill(&posted)),
    )
    .await;
    let reused = bill(&pool, &app, &cookie, &two, "ext-aa-001").await;
    assert_eq!(reused["supplier_id"], first["id"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_concurrent_retries_have_one_effect_and_bind_actor_body_and_operation(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-RETRY").await;
    let product = product(&app, &cookie, "AP-RETRY").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let body = bill_body(&pool, &received, "EXT-RETRY").await;
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", BILLS, Some(body.clone())),
        request(&app, &cookie, "POST", BILLS, Some(body.clone()))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(a, b);
    let posted = a.1;
    let before = snapshot(&pool).await;
    let (_, finance) = actor(&pool, "finance").await;
    assert_eq!(
        request(&app, &finance, "POST", BILLS, Some(body.clone()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut changed = body.clone();
    changed["notes"] = json!("Different notes");
    assert_eq!(
        request(&app, &cookie, "POST", BILLS, Some(changed)).await.0,
        StatusCode::CONFLICT
    );
    let mut cross = reverse_bill(&posted);
    cross["idempotency_key"] = body["idempotency_key"].clone();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_bill_path(&posted),
            Some(cross)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
    let path = reverse_bill_path(&posted);
    let reverse = reverse_bill(&posted);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(reverse.clone())),
        request(&app, &cookie, "POST", &path, Some(reverse))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(a, b);
    let updated = get_order(&app, &cookie, &received).await;
    let a = bill_body(&pool, &updated, "EXT-CONTENDED-A").await;
    let b = bill_body(&pool, &updated, "EXT-CONTENDED-B").await;
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", BILLS, Some(a)),
        request(&app, &cookie, "POST", BILLS, Some(b))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT),
        "{a:?} {b:?}"
    );
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM purchase_bills),(SELECT count(*) FROM journal_entries)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (2, 4));
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_authentication_and_finance_write_permissions_apply_to_all_routes(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-ROLE").await;
    let product = product(&app, &cookie, "AP-ROLE").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let body = bill_body(&pool, &received, "EXT-ROLE").await;
    for path in [
        BILLS.to_owned(),
        format!("{BILLS}/summary"),
        format!("{BILLS}/{}", Uuid::new_v4()),
    ] {
        assert_eq!(
            request(&app, "", "GET", &path, None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        request(&app, "", "POST", BILLS, Some(body.clone())).await.0,
        StatusCode::UNAUTHORIZED
    );
    for role in ["warehouse", "sales", "viewer"] {
        let (_, role_cookie) = actor(&pool, role).await;
        for path in [BILLS.to_owned(), format!("{BILLS}/summary")] {
            assert_eq!(
                request(&app, &role_cookie, "GET", &path, None).await.0,
                StatusCode::OK,
                "{role}: {path}"
            );
        }
        assert_eq!(
            request(&app, &role_cookie, "POST", BILLS, Some(body.clone()))
                .await
                .0,
            StatusCode::FORBIDDEN,
            "{role}"
        );
    }
    let (_, finance) = actor(&pool, "finance").await;
    let posted = call(&app, &finance, "POST", BILLS, Some(body)).await;
    for role in ["warehouse", "sales", "viewer"] {
        let (_, role_cookie) = actor(&pool, role).await;
        assert_eq!(
            request(
                &app,
                &role_cookie,
                "GET",
                &format!("{BILLS}/{}", id(&posted)),
                None
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                &role_cookie,
                "POST",
                &reverse_bill_path(&posted),
                Some(reverse_bill(&posted))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    call(
        &app,
        &finance,
        "POST",
        &reverse_bill_path(&posted),
        Some(reverse_bill(&posted)),
    )
    .await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "GET",
            &format!("{BILLS}/{}", Uuid::new_v4()),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_invalid_fields_dates_and_reversals_are_atomic_client_errors(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-INVALID").await;
    let product = product(&app, &cookie, "AP-INVALID").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let valid = bill_body(&pool, &received, "EXT-INVALID").await;
    for extra in [
        json!({"supplier_invoice_number":"  "}),
        json!({"supplier_invoice_number":"a".repeat(101)}),
        json!({"supplier_invoice_number":"bad\u{0000}number"}),
        json!({"supplier_invoice_number":"bad\nnumber"}),
        json!({"notes":"bad\u{0000}notes"}),
        json!({"invoice_date":"2026-02-30"}),
        json!({"invoice_date":"2026-1-01"}),
        json!({"invoice_date":"0000-01-01"}),
        json!({"invoice_date":"10000-01-01"}),
        json!({"invoice_date":"9999-12-31","due_date":"9999-12-31"}),
        json!({"invoice_date":"2020-01-02","due_date":"2020-01-01"}),
        json!({"due_date":"2026-13-01"}),
        json!({"total":"0.01"}),
        json!({"paid":"1"}),
        json!({"items":[]}),
        json!({"order_version":0}),
        json!({"receipt_id":Uuid::new_v4()}),
    ] {
        let mut invalid = valid.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let before = snapshot(&pool).await;
        let (status, value) = request(&app, &cookie, "POST", BILLS, Some(invalid.clone())).await;
        assert!(status.is_client_error(), "{invalid}: {status}: {value}");
        assert_eq!(snapshot(&pool).await, before, "Rejected input {invalid}");
    }
    // In the supported calendar the earliest ISO date is valid, including leap years.
    let mut oldest = valid.clone();
    oldest["invoice_date"] = json!("0001-01-01");
    oldest["due_date"] = json!("2000-02-29");
    let posted = call(&app, &cookie, "POST", BILLS, Some(oldest)).await;
    assert_eq!(posted["invoice_date"], "0001-01-01");
    let before = snapshot(&pool).await;
    for extra in [
        json!({"reason":"ab"}),
        json!({"reason":"a".repeat(1001)}),
        json!({"reason":"bad\u{0000}reason"}),
        json!({"version":0}),
        json!({"amount":"1"}),
    ] {
        let mut invalid = reverse_bill(&posted);
        invalid
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let (status, value) = request(
            &app,
            &cookie,
            "POST",
            &reverse_bill_path(&posted),
            Some(invalid.clone()),
        )
        .await;
        assert!(status.is_client_error(), "{invalid}: {status}: {value}");
        assert_eq!(snapshot(&pool).await, before);
    }
    let date:String=sqlx::query_scalar("SELECT (created_at AT TIME ZONE (SELECT timezone FROM company))::date::text FROM journal_entries WHERE id=$1")
        .bind(Uuid::parse_str(posted["journal_id"].as_str().unwrap()).unwrap()).fetch_one(&pool).await.unwrap();
    let today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date::text FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        date, today,
        "Invoice date is metadata; journal posting uses the current open local day"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_closed_period_blocks_both_post_and_reversal(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-CLOSED").await;
    let product = product(&app, &cookie, "AP-CLOSED").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let body = bill_body(&pool, &received, "EXT-CLOSED").await;
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let before = snapshot(&pool).await;
    assert_eq!(
        request(&app, &cookie, "POST", BILLS, Some(body.clone()))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
    sqlx::query("UPDATE accounting_period SET closed_through=NULL WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    let posted = call(&app, &cookie, "POST", BILLS, Some(body)).await;
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE (SELECT timezone FROM company))::date WHERE singleton").execute(&pool).await.unwrap();
    let before = snapshot(&pool).await;
    let read = call(
        &app,
        &cookie,
        "GET",
        &format!("{BILLS}/{}", id(&posted)),
        None,
    )
    .await;
    assert_eq!(read["can_reverse"], false);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_bill_path(&posted),
            Some(reverse_bill(&posted))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_active_bill_blocks_receipt_reversal_then_void_unlocks_without_rewinding_history(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-LOCK").await;
    let product = product(&app, &cookie, "AP-LOCK").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let posted = bill(&pool, &app, &cookie, &received, "EXT-LOCK").await;
    let before = snapshot(&pool).await;
    // Stale versions from a warehouse screen must not bypass the new finance document.
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_receipt_path(&received),
            Some(reverse_receipt(&received))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let updated = get_order(&app, &cookie, &received).await;
    assert_eq!(receipt(&updated)["can_reverse"], false);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &reverse_receipt_path(&updated),
            Some(reverse_receipt(&updated))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
    call(
        &app,
        &cookie,
        "POST",
        &reverse_bill_path(&posted),
        Some(reverse_bill(&posted)),
    )
    .await;
    let updated = get_order(&app, &cookie, &received).await;
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &reverse_receipt_path(&updated),
        Some(reverse_receipt(&updated)),
    )
    .await;
    assert_eq!(reversed["status"], "confirmed");
    let old_body = bill_body(&pool, &updated, "EXT-REVERSED-RECEIPT").await;
    assert_eq!(
        request(&app, &cookie, "POST", BILLS, Some(old_body))
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_later_stock_movement_does_not_prevent_billing_or_bill_reversal(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-LATER").await;
    let product = product(&app, &cookie, "AP-LATER").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    call(&app,&cookie,"POST","/api/inventory/adjustments",Some(json!({"product_id":product["id"],"quantity":"-0.125","reason":"Sampel barang yang sudah diterima","idempotency_key":Uuid::new_v4()}))).await;
    let updated = get_order(&app, &cookie, &received).await;
    assert_eq!(receipt(&updated)["can_reverse"], false);
    assert_eq!(receipt(&updated)["can_bill"], true);
    call(&app,&cookie,"PUT",&format!("/api/suppliers/{}",id(&supplier)),Some(json!({"version":supplier["version"],"code":supplier["code"],"name":"Supplier berhenti beroperasi","active":false,"idempotency_key":Uuid::new_v4()}))).await;
    let before = stock_and_operating_balances(&pool).await;
    let posted = bill(&pool, &app, &cookie, &updated, "EXT-LATER").await;
    assert_eq!(
        posted["supplier_name"], supplier["name"],
        "Billing received goods keeps the reviewed supplier snapshot even after deactivation"
    );
    call(
        &app,
        &cookie,
        "POST",
        &reverse_bill_path(&posted),
        Some(reverse_bill(&posted)),
    )
    .await;
    assert_eq!(stock_and_operating_balances(&pool).await, before);
    let updated = get_order(&app, &cookie, &received).await;
    assert_eq!(
        receipt(&updated)["can_reverse"],
        false,
        "Void removes only the bill dependency, never later stock movements"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_post_and_receipt_reversal_serialize_to_one_valid_winner(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AP-RACE").await;
    let product = product(&app, &cookie, "AP-RACE").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let body = bill_body(&pool, &received, "EXT-RACE").await;
    let path = reverse_receipt_path(&received);
    let reverse = reverse_receipt(&received);
    let (bill, reversal) = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        tokio::join!(
            request(&app, &cookie, "POST", BILLS, Some(body)),
            request(&app, &cookie, "POST", &path, Some(reverse))
        )
    })
    .await
    .expect("Concurrent billing and goods receipt reversal must not deadlock");
    assert!(
        (bill.0 == StatusCode::OK && reversal.0 == StatusCode::CONFLICT)
            || (reversal.0 == StatusCode::OK && bill.0 == StatusCode::CONFLICT),
        "{bill:?} {reversal:?}"
    );
    let invalid:i64=sqlx::query_scalar("SELECT count(*) FROM purchase_bills b JOIN posting_reversals r ON r.source_type='purchase_receipt' AND r.source_id=b.receipt_id WHERE b.status='unpaid'").fetch_one(&pool).await.unwrap();
    assert_eq!(invalid, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bill_summary_uses_company_calendar_and_all_bills_beyond_the_history_limit(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    // Choose a local calendar on the other side of midnight from UTC. This
    // catches a UTC-date implementation at any hour without freezing the clock.
    let timezone:String=sqlx::query_scalar("SELECT CASE WHEN extract(hour FROM now() AT TIME ZONE 'UTC')<12 THEN 'Etc/GMT+12' ELSE 'Pacific/Kiritimati' END").fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE company SET timezone=$1")
        .bind(&timezone)
        .execute(&pool)
        .await
        .unwrap();
    let dates:(String,String,String,String)=sqlx::query_as("SELECT (now() AT TIME ZONE timezone)::date::text,((now() AT TIME ZONE timezone)::date-1)::text,((now() AT TIME ZONE timezone)::date+1)::text,(now() AT TIME ZONE 'UTC')::date::text FROM company").fetch_one(&pool).await.unwrap();
    assert_ne!(dates.0, dates.3);
    let first = supplier(&app, &cookie, "AP-SUMMARY-A").await;
    let second = supplier(&app, &cookie, "AP-SUMMARY-B").await;
    let product = product(&app, &cookie, "AP-SUMMARY").await;
    let mut first_bill = Value::Null;
    for (i, (supplier, due)) in [
        (&first, &dates.0),
        (&first, &dates.1),
        (&second, &dates.2),
        (&second, &dates.1),
    ]
    .iter()
    .enumerate()
    {
        let order = received(&app, &cookie, supplier, &product).await;
        let mut body = bill_body(&pool, &order, &format!("EXT-SUMMARY-{i}")).await;
        body["due_date"] = json!(due);
        let posted = call(&app, &cookie, "POST", BILLS, Some(body)).await;
        assert_eq!(
            posted["due_status"],
            ["due_today", "overdue", "upcoming", "overdue"][i]
        );
        if i == 0 {
            first_bill = posted.clone();
        }
        if i == 3 {
            call(
                &app,
                &cookie,
                "POST",
                &reverse_bill_path(&posted),
                Some(reverse_bill(&posted)),
            )
            .await;
        }
    }
    // These historical fixtures need only headers and balanced financial
    // postings: the summary must query the entire payable position, independent
    // of the latest-500 document list or receipt line rendering.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO purchase_orders(id,number,supplier_id,supplier_code,supplier_name,total,created_by,created_at) SELECT md5('ap-summary-o-'||n)::uuid,'AP-SUMMARY-O-'||n,$1,'AP-SUMMARY-A','Supplier AP-SUMMARY-A',1,$2,now()-interval '2 years' FROM generate_series(1,501) n")
        .bind(uid(&first)).bind(admin).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) SELECT md5('ap-summary-rj-'||n)::uuid,'Historical summary receipt','purchase_receipt',md5('ap-summary-r-'||n)::uuid,$1,now()-interval '2 years' FROM generate_series(1,501) n UNION ALL SELECT md5('ap-summary-bj-'||n)::uuid,'Historical summary bill','purchase_bill',md5('ap-summary-b-'||n)::uuid,$1,now()-interval '2 years' FROM generate_series(1,501) n")
        .bind(admin).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT md5('ap-summary-rdr-'||n)::uuid,md5('ap-summary-rj-'||n)::uuid,'1300',1,0 FROM generate_series(1,501) n UNION ALL SELECT md5('ap-summary-rcr-'||n)::uuid,md5('ap-summary-rj-'||n)::uuid,(SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'),0,1 FROM generate_series(1,501) n UNION ALL SELECT md5('ap-summary-bdr-'||n)::uuid,md5('ap-summary-bj-'||n)::uuid,(SELECT code FROM accounts WHERE system_role='purchase_receipt_accrual'),1,0 FROM generate_series(1,501) n UNION ALL SELECT md5('ap-summary-bcr-'||n)::uuid,md5('ap-summary-bj-'||n)::uuid,(SELECT code FROM accounts WHERE system_role='purchase_payable'),0,1 FROM generate_series(1,501) n")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO purchase_receipts(id,number,order_id,total,journal_id,created_by,created_at) SELECT md5('ap-summary-r-'||n)::uuid,'AP-SUMMARY-R-'||n,md5('ap-summary-o-'||n)::uuid,1,md5('ap-summary-rj-'||n)::uuid,$1,now()-interval '2 years' FROM generate_series(1,501) n")
        .bind(admin).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE purchase_orders SET status='received',confirmed_at=created_at,received_at=created_at,active_receipt_id=md5('ap-summary-r-'||substr(number,14))::uuid WHERE number LIKE 'AP-SUMMARY-O-%'")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO purchase_bills(id,number,receipt_id,order_id,supplier_id,supplier_code,supplier_name,supplier_invoice_number,invoice_date,due_date,total,journal_id,created_by,created_at) SELECT md5('ap-summary-b-'||n)::uuid,'AP-SUMMARY-B-'||n,md5('ap-summary-r-'||n)::uuid,md5('ap-summary-o-'||n)::uuid,$1,'AP-SUMMARY-A','Supplier AP-SUMMARY-A','AP-SUMMARY-EXT-'||n,DATE '2020-01-01',$2::date,1,md5('ap-summary-bj-'||n)::uuid,$3,now()-interval '2 years' FROM generate_series(1,501) n")
        .bind(uid(&first)).bind(&dates.1).bind(admin).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let summary = call(&app, &cookie, "GET", &format!("{BILLS}/summary"), None).await;
    assert_eq!(summary["timezone"], timezone);
    assert_eq!(summary["today"], dates.0);
    assert!(summary["as_of"].is_string());
    assert_eq!(
        summary["counts"],
        json!({"unpaid":504,"void":1,"overdue":502,"due_today":1,"upcoming":1})
    );
    assert_eq!(amount(&summary, "outstanding_total"), d("542.67"));
    assert_eq!(amount(&summary, "overdue_total"), d("514.89"));
    assert_eq!(amount(&summary, "due_today_total"), d("13.89"));
    assert_eq!(amount(&summary, "upcoming_total"), d("13.89"));
    let groups = summary["suppliers"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["supplier_id"], first["id"]);
    assert_eq!(groups[0]["bill_count"], 503);
    assert_eq!(amount(&groups[0], "outstanding_total"), d("528.78"));
    assert_eq!(amount(&groups[0], "overdue_total"), d("514.89"));
    assert_eq!(amount(&groups[1], "outstanding_total"), d("13.89"));
    let list = call(&app, &cookie, "GET", BILLS, None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 500);
    let hidden = (1..=501)
        .map(|n| format!("AP-SUMMARY-B-{n}"))
        .find(|number| {
            !list["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["number"] == *number)
        })
        .unwrap();
    let hidden_id: Uuid = sqlx::query_scalar("SELECT id FROM purchase_bills WHERE number=$1")
        .bind(hidden)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(&app, &cookie, "GET", &format!("{BILLS}/{hidden_id}"), None)
            .await
            .0,
        StatusCode::OK
    );
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(amount(&summary, "supplier_payable_total"), d("542.67"));
    assert_eq!(amount(&summary, "received_unbilled_total"), d("13.89"));
    assert_eq!(first_bill["due_status"], "due_today");
}

#[sqlx::test(migrations = false)]
async fn supplier_bill_upgrade_preserves_custom_accounts_and_blocks_manual_control_posting(
    pool: PgPool,
) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|m| m.version <= 11)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES('2200','Utang khusus sebelum upgrade','liability')").execute(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let custom: (String, String, Option<String>) =
        sqlx::query_as("SELECT name,kind,system_role FROM accounts WHERE code='2200'")
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
    let payable: (String, String) =
        sqlx::query_as("SELECT code,kind FROM accounts WHERE system_role='purchase_payable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(payable, ("2201".into(), "liability".into()));
    let (app, _, cookie) = fixture(&pool).await;
    let accounts = call(&app, &cookie, "GET", "/api/accounting/accounts", None).await;
    let control = accounts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["code"] == payable.0)
        .unwrap();
    assert_eq!(control["is_control"], true);
    let lines = json!([{"account_code":"1100","debit":"10","credit":"0"},{"account_code":payable.0,"debit":"0","credit":"10"}]);
    let before = snapshot(&pool).await;
    assert!(request(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Direct payable credit must be rejected","lines":lines,"idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    assert_eq!(snapshot(&pool).await, before);
    assert!(request(&app,&cookie,"PUT","/api/accounting/opening",Some(json!({"version":0,"description":"Direct opening payable must be rejected","items":[],"lines":lines}))).await.0.is_client_error());
    let supplier = supplier(&app, &cookie, "AP-UPGRADE").await;
    let product = product(&app, &cookie, "AP-UPGRADE").await;
    let received = received(&app, &cookie, &supplier, &product).await;
    let posted = bill(&pool, &app, &cookie, &received, "EXT-UPGRADE").await;
    let credit: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(sum(credit-debit),0) FROM journal_lines WHERE account_code='2201'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(credit, amount(&posted, "total"));
}
