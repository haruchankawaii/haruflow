use crate::supplier_bill_tests::{actor, call, fixture, request, supplier};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
const OPENING: &str = "/api/accounting/opening";
fn uid(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}
fn row(kind: &str, id: Uuid, reference: &str, amount: &str) -> Value {
    json!({"kind":kind,"party_id":id,"reference":reference,"document_date":"2000-01-01","due_date":"2000-02-01","amount":amount})
}
async fn setup(pool: &PgPool) -> (axum::Router, String, Value, Value) {
    let (app, _, cookie) = fixture(pool).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Opening party customer"})),
    )
    .await;
    let supplier = supplier(&app, &cookie, "OPEN-PARTY").await;
    (app, cookie, customer, supplier)
}
fn draft(customer: Uuid, supplier: Uuid) -> Value {
    json!({"version":0,"description":"Migrasi buku usaha lama","items":[],"lines":[{"account_code":"1100","debit":"2000000","credit":"0"},{"account_code":"3000","debit":"0","credit":"1200000"}],"parties":[row("receivable",customer,"AR-OLD","2000000"),row("payable",supplier,"AP-OLD","3000000"),row("customer_credit",customer,"CUSTOMER-OLD-CREDIT","300000"),row("supplier_credit",supplier,"SUPPLIER-OLD-CREDIT","500000")]})
}
async fn post(app: &axum::Router, cookie: &str, value: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/accounting/opening/post",
        Some(json!({"version":value["document"]["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn cash(pool: &PgPool) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn assert_balanced(pool: &PgPool) {
    let invalid:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit))j").fetch_one(pool).await.unwrap();
    assert_eq!(invalid, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_parties_post_retry_credit_sources_and_partial_settlement_reconcile(pool: PgPool) {
    let (app, cookie, customer, supplier) = setup(&pool).await;
    let saved = call(
        &app,
        &cookie,
        "PUT",
        OPENING,
        Some(draft(uid(&customer), uid(&supplier))),
    )
    .await;
    assert_eq!(saved["document"]["parties"].as_array().unwrap().len(), 4);
    assert_eq!(saved["document"]["balanced"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM opening_party_items")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let input = json!({"version":saved["document"]["version"],"idempotency_key":Uuid::new_v4()});
    let (a, b) = tokio::join!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/opening/post",
            Some(input.clone())
        ),
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/opening/post",
            Some(input)
        )
    );
    assert!(a.0.is_success(), "{a:?}");
    assert_eq!(a, b);
    let posted = a.1;
    assert_eq!(posted["can_reverse"], true);
    let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT(SELECT count(*) FROM opening_party_items),(SELECT count(*) FROM party_funds),(SELECT count(*) FROM journal_entries),(SELECT count(*) FROM sales_orders)+(SELECT count(*) FROM purchase_orders)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (4, 2, 1, 0));
    let revenue:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(l.credit-l.debit),0) FROM journal_lines l JOIN accounts a ON a.code=l.account_code WHERE a.kind IN ('revenue','expense')").fetch_one(&pool).await.unwrap();
    assert_eq!(revenue, Decimal::ZERO);
    for (side, party, kind, amount, remaining) in [
        (
            "customer",
            uid(&customer),
            "receivable",
            "500000",
            "1500000",
        ),
        ("supplier", uid(&supplier), "payable", "1000000", "2000000"),
    ] {
        let id: Uuid = sqlx::query_scalar("SELECT id FROM opening_party_items WHERE kind=$1")
            .bind(kind)
            .fetch_one(&pool)
            .await
            .unwrap();
        let target = call(
            &app,
            &cookie,
            "GET",
            &format!("/api/opening-items/{id}"),
            None,
        )
        .await;
        let fund=call(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":side,"party_id":party,"amount":amount,"reference":"Pembayaran tagihan lama","idempotency_key":Uuid::new_v4()}))).await;
        call(&app,&cookie,"POST",&format!("/api/party-funds/{}/allocate",fund["id"].as_str().unwrap()),Some(json!({"version":fund["version"],"items":[{"target_type":target["target_type"],"target_id":id,"amount":amount,"version":target["version"]}],"idempotency_key":Uuid::new_v4()}))).await;
        let target = call(
            &app,
            &cookie,
            "GET",
            &format!("/api/opening-items/{id}"),
            None,
        )
        .await;
        assert_eq!(
            target["outstanding"]
                .as_str()
                .unwrap()
                .parse::<Decimal>()
                .unwrap(),
            remaining.parse::<Decimal>().unwrap()
        );
        assert_eq!(target["status"], "partial");
    }
    assert_eq!(cash(&pool).await, Decimal::from(1_500_000));
    assert_balanced(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(request(&app,&cookie,"POST","/api/accounting/opening/reverse",Some(json!({"version":posted["document"]["version"],"reason":"Sudah dibayar sebagian harus ditolak","idempotency_key":Uuid::new_v4()}))).await.0,409);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap(),
        before
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_parties_reverse_repost_retains_history_and_all_credits_reverse_without_cash(
    pool: PgPool,
) {
    let (app, cookie, customer, supplier) = setup(&pool).await;
    let saved = call(
        &app,
        &cookie,
        "PUT",
        OPENING,
        Some(draft(uid(&customer), uid(&supplier))),
    )
    .await;
    let posted = post(&app, &cookie, &saved).await;
    let reversed=call(&app,&cookie,"POST","/api/accounting/opening/reverse",Some(json!({"version":posted["document"]["version"],"reason":"Memperbaiki rincian pembukaan sebelum operasional","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(reversed["document"]["status"], "draft");
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    let available:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(b.available),0) FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b").fetch_one(&pool).await.unwrap();
    assert_eq!(available, Decimal::ZERO);
    let reposted = post(&app, &cookie, &reversed).await;
    assert_eq!(reposted["document"]["history"].as_array().unwrap().len(), 2);
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM opening_party_items")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 8);
    let balance:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(b.available),0) FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b").fetch_one(&pool).await.unwrap();
    assert_eq!(balance, Decimal::from(800_000));
    assert_balanced(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn opening_party_csv_preview_is_atomic_resolves_masters_and_rejects_controls_and_wrong_sides(
    pool: PgPool,
) {
    let (app, cookie, customer, supplier) = setup(&pool).await;
    let csv = "kind;party_id;party;reference;document_date;due_date;debit;credit\r\nreceivable;;Opening party customer;AR-OLD;2000-01-01;;2000000;\r\npayable;;OPEN-PARTY;AP-OLD;2000-01-01;2000-02-01;;3000000\r\n";
    let preview = call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/opening/parties/preview",
        Some(json!({"csv":csv,"delimiter":"semicolon"})),
    )
    .await;
    assert_eq!(preview["errors"], json!([]));
    assert_eq!(preview["parties"].as_array().unwrap().len(), 2);
    assert_eq!(preview["parties"][0]["party_id"], customer["id"]);
    assert_eq!(preview["parties"][1]["party_id"], supplier["id"]);
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"],
        Value::Null
    );
    for csv in [
        csv.replace(";;3000000", ";3000000;"),
        csv.replace("AR-OLD", "AR\0OLD"),
        csv.replace("AR-OLD", "\"unterminated"),
        csv.replace("OPEN-PARTY", "unknown-supplier"),
    ] {
        let result = call(
            &app,
            &cookie,
            "POST",
            "/api/accounting/opening/parties/preview",
            Some(json!({"csv":csv,"delimiter":"semicolon"})),
        )
        .await;
        assert!(!result["errors"].as_array().unwrap().is_empty());
        assert_eq!(result["parties"], json!([]));
    }
    let (_, warehouse) = actor(&pool, "warehouse").await;
    assert_eq!(
        request(
            &app,
            &warehouse,
            "POST",
            "/api/accounting/opening/parties/preview",
            Some(json!({"csv":csv,"delimiter":"semicolon"}))
        )
        .await
        .0,
        403
    );
    let mut invalid = draft(uid(&customer), uid(&supplier));
    invalid["parties"][0]["amount"] = json!("0");
    assert_eq!(
        request(&app, &cookie, "PUT", OPENING, Some(invalid))
            .await
            .0,
        400
    );
    let mut invalid = draft(uid(&customer), uid(&supplier));
    invalid["parties"][0]["document_date"] = json!("2000-02-30");
    assert_eq!(
        request(&app, &cookie, "PUT", OPENING, Some(invalid))
            .await
            .0,
        400
    );
    let mut invalid = draft(uid(&customer), uid(&supplier));
    let duplicate = invalid["parties"][0].clone();
    invalid["parties"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(
        request(&app, &cookie, "PUT", OPENING, Some(invalid))
            .await
            .0,
        400
    );
    assert_eq!(
        call(&app, &cookie, "GET", OPENING, None).await["document"],
        Value::Null
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
