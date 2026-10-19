use crate::supplier_bill_tests::{
    actor, bill, bill_body, call, fixture, get_order, product, received, request, snapshot,
    supplier,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn funded_bill(pool: &PgPool) -> (axum::Router, String, Value) {
    let (app, _, cookie) = fixture(pool).await;
    call(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Kas untuk pembayaran supplier","lines":[{"account_code":"1100","debit":"1000","credit":"0"},{"account_code":"3000","debit":"0","credit":"1000"}],"idempotency_key":Uuid::new_v4()}))).await;
    let supplier = supplier(&app, &cookie, "PAY").await;
    let product = product(&app, &cookie, "PAY").await;
    let order = received(&app, &cookie, &supplier, &product).await;
    let bill = bill(pool, &app, &cookie, &order, "PAY-001").await;
    (app, cookie, bill)
}
fn pay_path(bill: &Value) -> String {
    format!(
        "/api/purchase-bills/{}/payments",
        bill["id"].as_str().unwrap()
    )
}
fn pay(bill: &Value, amount: &str) -> Value {
    json!({"version":bill["version"],"amount":amount,"reference":"Bukti bayar uji","idempotency_key":Uuid::new_v4()})
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_payments_partial_full_retry_and_exact_reversal(pool: PgPool) {
    let (app, cookie, original) = funded_bill(&pool).await;
    let input = pay(&original, "5");
    let partial = call(
        &app,
        &cookie,
        "POST",
        &pay_path(&original),
        Some(input.clone()),
    )
    .await;
    assert_eq!(
        call(&app, &cookie, "POST", &pay_path(&original), Some(input)).await,
        partial
    );
    assert_eq!(partial["status"], "partial");
    assert_eq!(partial["paid"], "5.00");
    assert_eq!(partial["outstanding"], "8.89");
    assert_eq!(partial["can_reverse"], false);
    let paid = call(
        &app,
        &cookie,
        "POST",
        &pay_path(&partial),
        Some(pay(&partial, "8.89")),
    )
    .await;
    assert_eq!(paid["status"], "paid");
    assert_eq!(paid["due_status"], "paid");
    assert_eq!(paid["outstanding"], "0.00");
    assert_eq!(paid["payments"].as_array().unwrap().len(), 2);
    let summary = call(&app, &cookie, "GET", "/api/purchase-bills/summary", None).await;
    assert_eq!(summary["counts"]["paid"], 1);
    assert_eq!(summary["outstanding_total"], "0");
    let payment = &paid["payments"][1];
    let path = format!(
        "/api/supplier-payments/{}/reverse",
        payment["id"].as_str().unwrap()
    );
    let input = json!({"version":paid["version"],"reason":"Koreksi nominal pembayaran","idempotency_key":Uuid::new_v4()});
    let reversed = call(&app, &cookie, "POST", &path, Some(input.clone())).await;
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(input)).await,
        reversed
    );
    assert_eq!(reversed["paid"], "5.00");
    assert_eq!(reversed["status"], "partial");
    let cash: String = sqlx::query_scalar(
        "SELECT sum(debit-credit)::text FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cash, "995.00");
    let analysis = call(
        &app,
        &cookie,
        "GET",
        "/api/purchase-bills/analysis?months=3",
        None,
    )
    .await;
    assert_eq!(analysis["series"].as_array().unwrap().len(), 3);
    assert_eq!(analysis["series"][2]["net"], "5.00");
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_payments_reject_overpay_roles_controls_closed_period_without_partial_writes(
    pool: PgPool,
) {
    let (app, cookie, original) = funded_bill(&pool).await;
    let before = snapshot(&pool).await;
    for amount in ["0", "-1", "14", "0.001", "1\0"] {
        assert!(
            request(
                &app,
                &cookie,
                "POST",
                &pay_path(&original),
                Some(pay(&original, amount))
            )
            .await
            .0
            .is_client_error()
        );
        assert_eq!(snapshot(&pool).await, before);
    }
    let (_, viewer) = actor(&pool, "warehouse").await;
    assert_eq!(
        request(
            &app,
            &viewer,
            "POST",
            &pay_path(&original),
            Some(pay(&original, "1"))
        )
        .await
        .0,
        403
    );
    let input = pay(&original, "1");
    let paid = call(&app, &cookie, "POST", &pay_path(&original), Some(input)).await;
    assert!(request(&app,&cookie,"POST",&format!("/api/purchase-bills/{}/reverse",paid["id"].as_str().unwrap()),Some(json!({"version":paid["version"],"reason":"Harus ditolak ada pembayaran","idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    sqlx::query(
        "UPDATE accounting_period SET closed_through=(now() AT TIME ZONE 'Asia/Jakarta')::date",
    )
    .execute(&pool)
    .await
    .unwrap();
    let before = snapshot(&pool).await;
    assert!(
        request(
            &app,
            &cookie,
            "POST",
            &pay_path(&paid),
            Some(pay(&paid, "1"))
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(snapshot(&pool).await, before);
    let payment = &paid["payments"][0];
    assert!(request(&app,&cookie,"POST",&format!("/api/supplier-payments/{}/reverse",payment["id"].as_str().unwrap()),Some(json!({"version":paid["version"],"reason":"Periode harus terbuka","idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_payments_competing_keys_and_insufficient_cash_are_atomic(pool: PgPool) {
    let (app, cookie, original) = funded_bill(&pool).await;
    let input = pay(&original, "10");
    let path = pay_path(&original);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(input.clone())),
        request(&app, &cookie, "POST", &path, Some(input.clone()))
    );
    assert!(a.0.is_success());
    assert_eq!(a.1, b.1);
    let (_, other) = actor(&pool, "finance").await;
    assert_eq!(
        request(&app, &other, "POST", &pay_path(&original), Some(input))
            .await
            .0,
        409
    );
    let current = a.1;
    let mut tx = pool.begin().await.unwrap();
    let jid = Uuid::new_v4();
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE role='admin' LIMIT 1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Cash withdrawn','manual',$1,$2)").bind(jid).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1100',0,990),($3,$2,'3000',990,0)").bind(Uuid::new_v4()).bind(jid).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &pay_path(&current),
            Some(pay(&current, "1"))
        )
        .await
        .0,
        409
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_bills_split_quantities_round_to_original_and_refuse_overallocation(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "SPLIT").await;
    let product = product(&app, &cookie, "SPLIT").await;
    let order = received(&app, &cookie, &supplier, &product).await;
    let mut input = bill_body(&pool, &order, "SPLIT-1").await;
    input["items"] =
        json!([{"receipt_line_id":order["receipts"][0]["items"][0]["id"],"quantity":"0.5"}]);
    let first = call(&app, &cookie, "POST", "/api/purchase-bills", Some(input)).await;
    assert_eq!(first["total"], "6.17");
    let order = get_order(&app, &cookie, &order).await;
    let mut over = bill_body(&pool, &order, "SPLIT-OVER").await;
    over["items"] =
        json!([{"receipt_line_id":order["receipts"][0]["items"][0]["id"],"quantity":"1"}]);
    assert_eq!(
        request(&app, &cookie, "POST", "/api/purchase-bills", Some(over))
            .await
            .0,
        409
    );
    let last = bill(&pool, &app, &cookie, &order, "SPLIT-2").await;
    assert_eq!(last["total"], "7.72");
    assert_eq!(last["items"][0]["quantity"], "0.625");
    let summary = call(&app, &cookie, "GET", "/api/purchasing/summary", None).await;
    assert_eq!(summary["received_unbilled_total"], "0.00");
}
