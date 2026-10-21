use crate::supplier_bill_tests::{actor, call, fixture, request};
use axum::Router;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::borrow::Cow;
use uuid::Uuid;

fn uid(value: &Value) -> Uuid {
    value["id"].as_str().unwrap().parse().unwrap()
}
fn money(value: &Value, key: &str) -> Decimal {
    value[key].as_str().unwrap().parse().unwrap()
}
fn action(value: &Value) -> Value {
    json!({"version":value["version"],"reason":"Koreksi penyelesaian pihak","idempotency_key":Uuid::new_v4()})
}
async fn customer(app: &Router, cookie: &str) -> Uuid {
    uid(&call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Pelanggan saldo pihak"})),
    )
    .await)
}
async fn seed_invoice(
    pool: &PgPool,
    user: Uuid,
    customer: Uuid,
    total: i64,
    days_ago: i32,
) -> Uuid {
    let id = Uuid::new_v4();
    let order = Uuid::new_v4();
    let journal = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO sales_orders(id,number,customer_id,total,created_by) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(order)
    .bind(format!("SO-{order}"))
    .bind(customer)
    .bind(Decimal::from(total))
    .bind(user)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO invoices(id,number,order_id,customer_id,total,created_at) VALUES($1,$2,$3,$4,$5,now()-$6::int*interval '1 day')").bind(id).bind(format!("INV-{id}")).bind(order).bind(customer).bind(Decimal::from(total)).bind(days_ago).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) VALUES($1,'Fixture invoice','invoice',$2,$3,now()-$4::int*interval '1 day')").bind(journal).bind(id).bind(user).bind(days_ago).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1200',$3,0),($4,$2,'4000',0,$3)").bind(Uuid::new_v4()).bind(journal).bind(Decimal::from(total)).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    id
}
async fn fund(app: &Router, cookie: &str, customer: Uuid, value: &str) -> Value {
    call(app,cookie,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":customer,"amount":value,"reference":"Transfer nyata","description":"Uang muka","idempotency_key":Uuid::new_v4()}))).await
}
async fn allocate(app: &Router, cookie: &str, fund: &Value, invoice: Uuid, value: &str) -> Value {
    call(app,cookie,"POST",&format!("/api/party-funds/{}/allocate",uid(fund)),Some(json!({"version":fund["version"],"items":[{"target_type":"invoice","target_id":invoice,"amount":value,"version":1}],"idempotency_key":Uuid::new_v4()}))).await
}
async fn cash(pool: &PgPool) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_advance_allocation_reservations_partial_refunds_and_dependency_reversal_reconcile(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let party = customer(&app, &cookie).await;
    let invoice = seed_invoice(&pool, admin, party, 600000, 0).await;
    let original = fund(&app, &cookie, party, "1000000").await;
    let fid = uid(&original);
    assert_eq!(cash(&pool).await, Decimal::from(1000000));
    let applied = allocate(&app, &cookie, &original, invoice, "600000").await;
    assert_eq!(money(&applied, "available"), Decimal::from(400000));
    assert_eq!(
        cash(&pool).await,
        Decimal::from(1000000),
        "Allocation must not receive cash twice"
    );
    let reserved=call(&app,&cookie,"POST",&format!("/api/party-funds/{fid}/refund-requests"),Some(json!({"version":applied["version"],"amount":"300000","reason":"Refund diminta pelanggan","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(money(&reserved, "available"), Decimal::from(100000));
    assert_eq!(money(&reserved, "reserved"), Decimal::from(300000));
    let request_id = uid(&reserved["refund_requests"][0]);
    let paid=call(&app,&cookie,"POST",&format!("/api/party-refund-requests/{request_id}/pay"),Some(json!({"version":reserved["version"],"amount":"200000","reason":"Transfer sebagian selesai","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(cash(&pool).await, Decimal::from(800000));
    assert_eq!(money(&paid, "reserved"), Decimal::from(100000));
    assert_eq!(money(&paid, "available"), Decimal::from(100000));
    let cancelled = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/party-refund-requests/{request_id}/cancel"),
        Some(action(&paid)),
    )
    .await;
    assert_eq!(money(&cancelled, "available"), Decimal::from(200000));
    let refunded=call(&app,&cookie,"POST",&format!("/api/party-funds/{fid}/refunds"),Some(json!({"version":cancelled["version"],"amount":"200000","reason":"Sisa refund selesai","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(money(&refunded, "available"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, Decimal::from(600000));
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("/api/party-funds/{fid}/reverse"),
            Some(action(&refunded))
        )
        .await
        .0,
        409
    );
    let mut current = refunded.clone();
    for refund in refunded["refunds"].as_array().unwrap() {
        current = call(
            &app,
            &cookie,
            "POST",
            &format!("/api/party-refunds/{}/reverse", uid(refund)),
            Some(action(&current)),
        )
        .await;
    }
    assert_eq!(money(&current, "available"), Decimal::from(400000));
    current = call(
        &app,
        &cookie,
        "POST",
        &format!(
            "/api/party-allocations/{}/reverse",
            uid(&current["allocations"][0])
        ),
        Some(action(&current)),
    )
    .await;
    assert_eq!(money(&current, "available"), Decimal::from(1000000));
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/party-funds/{fid}/reverse"),
        Some(action(&current)),
    )
    .await;
    assert_eq!(reversed["reversed"], true);
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    let statement = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(
        money(&statement["summary"], "outstanding"),
        Decimal::from(600000)
    );
    assert_eq!(
        money(&statement["summary"], "credit_balance"),
        Decimal::ZERO
    );
    assert_eq!(statement["reconciliation"]["balanced"], true);
    let refund_events: Vec<_> = statement["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(
                event["event_type"].as_str(),
                Some("refund" | "refund_reversal")
            )
        })
        .collect();
    assert_eq!(refund_events.len(), 4);
    for event in refund_events {
        assert_eq!(event["fund_id"], fid.to_string());
        assert_eq!(event["source_url"], format!("?fund={fid}#balances"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_allocations_retry_permissions_and_cross_party_checks_do_not_overspend(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let party = customer(&app, &cookie).await;
    let other = customer(&app, &cookie).await;
    let first = seed_invoice(&pool, admin, party, 600, 0).await;
    let second = seed_invoice(&pool, admin, party, 600, 0).await;
    let foreign = seed_invoice(&pool, admin, other, 100, 0).await;
    let body = json!({"side":"customer","party_id":party,"amount":"1000","idempotency_key":Uuid::new_v4()});
    let value = call(
        &app,
        &cookie,
        "POST",
        "/api/party-funds",
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        call(&app, &cookie, "POST", "/api/party-funds", Some(body)).await,
        value
    );
    let path = format!("/api/party-funds/{}/allocate", uid(&value));
    let wrong = json!({"version":1,"items":[{"target_type":"invoice","target_id":foreign,"amount":"100","version":1}],"idempotency_key":Uuid::new_v4()});
    assert_eq!(
        request(&app, &cookie, "POST", &path, Some(wrong)).await.0,
        400
    );
    let a = json!({"version":1,"items":[{"target_type":"invoice","target_id":first,"amount":"600","version":1}],"idempotency_key":Uuid::new_v4()});
    let b = json!({"version":1,"items":[{"target_type":"invoice","target_id":second,"amount":"600","version":1}],"idempotency_key":Uuid::new_v4()});
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(a)),
        request(&app, &cookie, "POST", &path, Some(b))
    );
    assert!(
        matches!((a.0.as_u16(), b.0.as_u16()), (200, 409) | (409, 200)),
        "{a:?}/{b:?}"
    );
    assert_eq!(cash(&pool).await, Decimal::from(1000));
    let (_, viewer) = actor(&pool, "viewer").await;
    assert_eq!(request(&app,&viewer,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":party,"amount":"10","idempotency_key":Uuid::new_v4()}))).await.0,403);
    call(
        &app,
        &viewer,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    let current = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/party-funds/{}", uid(&value)),
        None,
    )
    .await;
    assert_eq!(money(&current, "available"), Decimal::from(400));
    assert_eq!(current["description"], Value::Null);
    let today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Jakarta')::date::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/period/close",
        Some(
            json!({"through":today,"reason":"Period test close","idempotency_key":Uuid::new_v4()}),
        ),
    )
    .await;
    assert_eq!(request(&app,&cookie,"POST",&format!("/api/party-funds/{}/refund-requests",uid(&value)),Some(json!({"version":current["version"],"amount":"100","reason":"Closed refund","idempotency_key":Uuid::new_v4()}))).await.0,409);
}

#[sqlx::test(migrations = "../../migrations")]
async fn statement_as_of_ignores_later_allocations_and_unknown_legacy_due_dates_stay_unknown(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let party = customer(&app, &cookie).await;
    let invoice = seed_invoice(&pool, admin, party, 1000, 2).await;
    let advance = fund(&app, &cookie, party, "1500").await;
    allocate(&app, &cookie, &advance, invoice, "1000").await;
    let yesterday: String =
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'Asia/Jakarta')::date-1)::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    let past = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement?as_of={yesterday}"),
        None,
    )
    .await;
    assert_eq!(money(&past["summary"], "outstanding"), Decimal::from(1000));
    assert_eq!(money(&past["summary"], "credit_balance"), Decimal::ZERO);
    assert_eq!(past["documents"][0]["bucket"], "unknown");
    assert_eq!(past["documents"][0]["due_date"], Value::Null);
    assert_eq!(past["reconciliation"]["balanced"], true);
    let now = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(money(&now["summary"], "outstanding"), Decimal::ZERO);
    assert_eq!(money(&now["summary"], "credit_balance"), Decimal::from(500));
    assert_eq!(now["reconciliation"]["balanced"], true);
}

#[sqlx::test(migrations = false)]
async fn settlement_upgrade_preserves_legacy_cash_and_journal_rows_without_funding_it_again(
    pool: PgPool,
) {
    let mut old = sqlx::migrate!("../../migrations");
    old.migrations = Cow::Owned(old.iter().filter(|m| m.version <= 21).cloned().collect());
    old.run(&pool).await.unwrap();
    let (app, admin, cookie) = fixture(&pool).await;
    let party = customer(&app, &cookie).await;
    let invoice = seed_invoice(&pool, admin, party, 1000, 2).await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/invoices/{invoice}/payments"),
        Some(json!({"amount":"500","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('payments',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM payments p),'journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l))").fetch_one(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('payments',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM payments p),'journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l))").fetch_one(&pool).await.unwrap();
    assert_eq!(after, before);
    assert_eq!(cash(&pool).await, Decimal::from(500));
    let funds = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/party-funds?side=customer&party_id={party}"),
        None,
    )
    .await;
    assert_eq!(funds["items"], json!([]));
    let statement = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(
        money(&statement["summary"], "outstanding"),
        Decimal::from(500)
    );
    assert_eq!(statement["reconciliation"]["balanced"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_dates_customer_terms_history_and_cash_vs_applied_credit_are_distinct(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let party = uid(&call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Customer thirty days","payment_terms_days":30})),
    )
    .await);
    let invoice = seed_invoice(&pool, admin, party, 1000, 0).await;
    let original = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/invoices/{invoice}"),
        None,
    )
    .await;
    let today: chrono::NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Jakarta')::date")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original["invoice_date"], today.to_string());
    assert_eq!(
        original["due_date"],
        (today + chrono::Duration::days(30)).to_string()
    );
    let journal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    let changed=call(&app,&cookie,"PATCH",&format!("/api/invoices/{invoice}/terms"),Some(json!({"version":original["version"],"due_date":(today+chrono::Duration::days(60)).to_string(),"reason":"Jangka pembayaran disepakati","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(changed["invoice_date"], today.to_string());
    assert_eq!(
        changed["due_date"],
        (today + chrono::Duration::days(60)).to_string()
    );
    assert_eq!(changed["terms_history"].as_array().unwrap().len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap(),
        journal_count,
        "Terms metadata must not post financial amounts"
    );
    call(&app,&cookie,"PATCH",&format!("/api/customers/{party}/terms"),Some(json!({"previous_payment_terms_days":30,"payment_terms_days":45,"reason":"Syarat pesanan berikutnya","idempotency_key":Uuid::new_v4()}))).await;
    let second = seed_invoice(&pool, admin, party, 500, 0).await;
    let new = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/invoices/{second}"),
        None,
    )
    .await;
    assert_eq!(
        new["due_date"],
        (today + chrono::Duration::days(45)).to_string()
    );
    let received = fund(&app, &cookie, party, "1000").await;
    let result=call(&app,&cookie,"POST",&format!("/api/party-funds/{}/allocate",uid(&received)),Some(json!({"version":received["version"],"items":[{"target_type":"invoice","target_id":invoice,"amount":"1000","version":changed["version"]}],"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(money(&result, "available"), Decimal::ZERO);
    let document = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/invoices/{invoice}"),
        None,
    )
    .await;
    assert_eq!(money(&document, "cash_paid"), Decimal::ZERO);
    assert_eq!(money(&document, "allocated_paid"), Decimal::from(1000));
    assert_eq!(money(&document, "credit_transferred"), Decimal::ZERO);
    assert_eq!(money(&document, "settled_total"), Decimal::from(1000));
    assert!(
        sqlx::query("UPDATE party_allocations SET amount=amount+1 WHERE fund_id=$1")
            .bind(uid(&received))
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE party_funds SET amount=amount+1,version=version+1 WHERE id=$1")
            .bind(uid(&received))
            .execute(&pool)
            .await
            .is_err()
    );
}
