use crate::supplier_bill_tests::{call, fixture, request, supplier};
use axum::{Router, http::StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap()
}

async fn paid_service_invoice(app: &Router, cookie: &str) -> (Value, Value) {
    let customer = call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Pelanggan pengujian kas"})),
    )
    .await;
    let service = call(
        app,
        cookie,
        "POST",
        "/api/products",
        Some(json!({"sku":"CASH-SAFETY","name":"Jasa pengujian kas","kind":"service","unit":"unit","price":"100"})),
    )
    .await;
    let order = call(
        app,
        cookie,
        "POST",
        "/api/orders",
        Some(json!({"customer_id":customer["id"],"items":[{"product_id":service["id"],"quantity":"1","unit_price":"100"}],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/orders/{}/confirm", id(&order)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let invoice = call(
        app,
        cookie,
        "POST",
        &format!("/api/orders/{}/invoice", id(&order)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let paid = call(
        app,
        cookie,
        "POST",
        &format!("/api/invoices/{}/payments", id(&invoice)),
        Some(json!({"amount":"100","reference":"Kas diterima","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let payments = call(
        app,
        cookie,
        "GET",
        &format!("/api/invoices/{}/payments", id(&invoice)),
        None,
    )
    .await;
    (paid, payments["items"][0].clone())
}

async fn cash(pool: &PgPool) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn financial_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
        'invoices',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM invoices x),
        'payments',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM payments x),
        'funds',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM party_funds x),
        'allocations',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM party_allocations x),
        'refunds',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM party_refunds x),
        'reversals',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM posting_reversals x),
        'journals',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM journal_entries x),
        'lines',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM journal_lines x),
        'audit',(SELECT jsonb_agg(to_jsonb(x) ORDER BY id) FROM audit_events x),
        'requests',(SELECT jsonb_agg(to_jsonb(x) ORDER BY key) FROM idempotency_keys x))"#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn spent_invoice_receipts_cannot_be_reversed_until_cash_is_restored(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let (invoice, payment) = paid_service_invoice(&app, &cookie).await;
    let supplier = supplier(&app, &cookie, "CASH-SAFETY").await;
    let advance = call(
        &app,
        &cookie,
        "POST",
        "/api/party-funds",
        Some(json!({"side":"supplier","party_id":supplier["id"],"amount":"100","reference":"Kas dibayar","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    let path = format!(
        "/api/invoices/{}/payments/{}/reverse",
        id(&invoice),
        id(&payment)
    );
    let body = json!({"reason":"Koreksi penerimaan pelanggan","idempotency_key":Uuid::new_v4()});
    let before = financial_snapshot(&pool).await;
    let rejected = request(&app, &cookie, "POST", &path, Some(body.clone())).await;
    assert_eq!(rejected.0, StatusCode::CONFLICT, "{rejected:?}");
    assert_eq!(financial_snapshot(&pool).await, before);
    assert_eq!(cash(&pool).await, Decimal::ZERO);

    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/party-funds/{}/reverse", id(&advance)),
        Some(json!({"version":advance["version"],"reason":"Koreksi pembayaran supplier","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(cash(&pool).await, Decimal::from(100));
    let reversed = call(&app, &cookie, "POST", &path, Some(body.clone())).await;
    assert_eq!(reversed["status"], "unpaid");
    assert_eq!(
        reversed["paid"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::ZERO
    );
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    let after = financial_snapshot(&pool).await;
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(body)).await,
        reversed
    );
    assert_eq!(financial_snapshot(&pool).await, after);
    let balances: Vec<Decimal> =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines GROUP BY journal_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(balances.iter().all(Decimal::is_zero));
}
