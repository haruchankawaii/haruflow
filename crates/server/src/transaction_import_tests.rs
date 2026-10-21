use crate::supplier_bill_tests::{actor, call, fixture, request, supplier};
use axum::{Router, http::StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
const ROOT: &str = "/api/transaction-imports";
async fn preview(app: &Router, cookie: &str, kind: &str, csv: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("{ROOT}/{kind}/preview"),
        Some(json!({"csv":csv,"delimiter":","})),
    )
    .await
}
async fn commit(app: &Router, cookie: &str, preview: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("{ROOT}/{}/commit", preview["id"].as_str().unwrap()),
        Some(json!({"confirmed":true,"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn customer(app: &Router, cookie: &str, name: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":name})),
    )
    .await
}
async fn counts(pool: &PgPool) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM historical_documents),(SELECT count(*) FROM historical_payments),(SELECT count(*) FROM journal_entries)").fetch_one(pool).await.unwrap()
}
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_invoice_payment_subledger_and_reversal_preserve_dates_and_stock(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let c = customer(&app, &cookie, "Historical customer").await;
    let party = c["id"].as_str().unwrap();
    let csv = format!(
        "reference,party_id,document_date,due_date,total,counter_account,description\nH-INV-1,{party},2020-01-02,2020-02-02,100,4000,Invoice lama\nH-INV-2,{party},2020-01-03,2020-02-03,50,4000,Invoice kedua\n"
    );
    let p = preview(&app, &cookie, "customer_invoices", &csv).await;
    assert_eq!(p["preview"]["error_count"], 0);
    let key = Uuid::new_v4();
    let path = format!("{ROOT}/{}/commit", p["id"].as_str().unwrap());
    let input = json!({"confirmed":true,"idempotency_key":key});
    let posted = call(&app, &cookie, "POST", &path, Some(input.clone())).await;
    assert_eq!(posted["count"], 2);
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(input)).await,
        posted
    );
    let doc = posted["items"][0]["id"].as_str().unwrap();
    let payment_csv = format!(
        "reference,document_reference,party_id,payment_date,amount,cash_account_code\nH-PAY-1,H-INV-1,{party},2020-01-10,40,1100\nH-PAY-2,H-INV-1,{party},2020-01-11,20,1100\n"
    );
    let preview_payment = preview(&app, &cookie, "customer_payments", &payment_csv).await;
    assert_eq!(preview_payment["preview"]["error_count"], 0);
    commit(&app, &cookie, &preview_payment).await;
    let state = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/historical-documents/{doc}"),
        None,
    )
    .await;
    assert_eq!(state["paid"], "60.00");
    assert_eq!(state["outstanding"], "40.00");
    assert_eq!(state["can_reverse"], false);
    assert!(
        state["created_at"]
            .as_str()
            .unwrap()
            .starts_with("2020-01-02")
    );
    assert!(!state["recorded_at"].as_str().unwrap().starts_with("2020"));
    let ar: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1200'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ar, d("90"));
    let statement = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(statement["summary"]["outstanding"], "90.00");
    assert_eq!(statement["reconciliation"]["balanced"], true);
    let past = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement?as_of=2020-01-10"),
        None,
    )
    .await;
    assert_eq!(past["summary"]["outstanding"], "110.00");
    assert_eq!(past["reconciliation"]["balanced"], true);
    let old_events:Decimal=sqlx::query_scalar("SELECT sum(amount) FROM historical_party_events WHERE side='customer' AND created_at<'2020-01-11T00:00:00Z'").fetch_one(&pool).await.unwrap();
    assert_eq!(old_events, d("110"));
    let reverse_doc = format!("/api/historical-documents/{doc}/reverse");
    assert_eq!(request(&app,&cookie,"POST",&reverse_doc,Some(json!({"version":state["version"],"reason":"Koreksi dokumen","idempotency_key":Uuid::new_v4()}))).await.0,StatusCode::CONFLICT);
    let mut latest = state;
    for index in [1, 0] {
        let payment = latest["payments"][index]["id"].as_str().unwrap();
        latest=call(&app,&cookie,"POST",&format!("/api/historical-payments/{payment}/reverse"),Some(json!({"version":latest["version"],"reason":"Koreksi pembayaran","idempotency_key":Uuid::new_v4()}))).await;
    }
    let void=call(&app,&cookie,"POST",&reverse_doc,Some(json!({"version":latest["version"],"reason":"Dokumen lama ganda","idempotency_key":Uuid::new_v4()}))).await;
    assert!(void["reversal"].is_object());
    assert_eq!(void["outstanding"], "0");
    let movements: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_movements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(movements, 0);
    let unbalanced:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit))x").fetch_one(&pool).await.unwrap();
    assert_eq!(unbalanced, 0);
    let historical: Decimal = sqlx::query_scalar(
        "SELECT sum(amount) FROM historical_party_events WHERE created_at<'2021-01-01'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(historical, d("90"));
    let current = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(current["summary"]["outstanding"], "50.00");
    assert_eq!(current["reconciliation"]["balanced"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_document_accepts_same_party_credit_allocation_without_second_cash(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let c = customer(&app, &cookie, "Historical allocation customer").await;
    let party = c["id"].as_str().unwrap();
    let preview=preview(&app,&cookie,"customer_invoices",&format!("reference,party_id,document_date,due_date,total,counter_account,description\nALLOC-HIST,{party},2020-01-01,2020-02-01,100,4000,Piutang historis\n")).await;
    let imported = commit(&app, &cookie, &preview).await;
    let document = imported["items"][0]["id"].as_str().unwrap();
    let fund=call(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":party,"amount":"30","reference":"Dana nyata","description":"Uang muka untuk invoice historis","idempotency_key":Uuid::new_v4()}))).await;
    let allocated=call(&app,&cookie,"POST",&format!("/api/party-funds/{}/allocate",fund["id"].as_str().unwrap()),Some(json!({"version":fund["version"],"items":[{"target_type":"historical_receivable","target_id":document,"version":1,"amount":"30"}],"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(allocated["available"], "0.00");
    let doc = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/historical-documents/{document}"),
        None,
    )
    .await;
    assert_eq!(doc["paid"], "30.00");
    assert_eq!(doc["outstanding"], "70.00");
    assert_eq!(doc["can_reverse"], false);
    let cash: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1100'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cash, d("30"));
    let statement = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/parties/customer/{party}/statement"),
        None,
    )
    .await;
    assert_eq!(statement["summary"]["outstanding"], "70.00");
    assert_eq!(statement["reconciliation"]["balanced"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_import_revalidates_atomic_batches_roles_duplicates_and_closed_dates(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let c = customer(&app, &cookie, "Atomic historical customer").await;
    let party = c["id"].as_str().unwrap();
    let csv = format!(
        "reference,party_id,document_date,due_date,total,counter_account,description\nFIRST,{party},2020-01-02,2020-02-02,100,4000,Valid\nFAIL-LATE,{party},2020-01-03,2020-02-03,50,4000,Valid too\n"
    );
    let p = preview(&app, &cookie, "customer_invoices", &csv).await;
    assert_eq!(p["preview"]["error_count"], 0);
    sqlx::query("CREATE FUNCTION reject_late_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.reference='FAIL-LATE' THEN RAISE EXCEPTION 'test late failure'; END IF;RETURN NEW;END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_late_history BEFORE INSERT ON historical_documents FOR EACH ROW EXECUTE FUNCTION reject_late_history()").execute(&pool).await.unwrap();
    let before = counts(&pool).await;
    let path = format!("{ROOT}/{}/commit", p["id"].as_str().unwrap());
    let (status, _) = request(
        &app,
        &cookie,
        "POST",
        &path,
        Some(json!({"confirmed":true,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert!(status.is_server_error());
    assert_eq!(counts(&pool).await, before);
    sqlx::query("DROP TRIGGER reject_late_history ON historical_documents")
        .execute(&pool)
        .await
        .unwrap();
    let (_, viewer) = actor(&pool, "viewer").await;
    assert_eq!(
        request(&app, &viewer, "GET", ROOT, None).await.0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE accounting_period SET closed_through='2020-12-31',version=version+1 WHERE singleton").execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path,
            Some(json!({"confirmed":true,"idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(counts(&pool).await, before);
    let blocked = preview(&app, &cookie, "customer_invoices", &csv).await;
    assert_eq!(blocked["preview"]["can_commit"], false);
    assert_eq!(blocked["preview"]["error_count"], 2);
    sqlx::query(
        "UPDATE accounting_period SET closed_through=NULL,version=version+1 WHERE singleton",
    )
    .execute(&pool)
    .await
    .unwrap();
    let p = preview(&app, &cookie, "customer_invoices", &csv).await;
    commit(&app, &cookie, &p).await;
    let duplicate = preview(&app, &cookie, "customer_invoices", &csv).await;
    assert_eq!(duplicate["preview"]["can_commit"], false);
    let bad = format!(
        "reference,party_id,document_date,due_date,total,counter_account,description\nBAD,{party},2020-01-01,2020-02-01,1,1100,Cash counter forbidden\nBAD,{party},2020-01-01,2020-02-01,1,4000,Duplicate CSV reference\n"
    );
    assert_eq!(
        preview(&app, &cookie, "customer_invoices", &bad).await["preview"]["error_count"],
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_supplier_payments_enforce_party_overpay_and_historical_cash(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let s = supplier(&app, &cookie, "HIST-SUP").await;
    let party = s["id"].as_str().unwrap();
    let p=preview(&app,&cookie,"supplier_bills",&format!("reference,party_id,document_date,due_date,total,counter_account,description\nH-BILL,{party},2020-01-01,2020-02-01,100,5000,Beban supplier lama\n")).await;
    assert_eq!(p["preview"]["error_count"], 0);
    commit(&app, &cookie, &p).await;
    let payments = format!(
        "reference,document_reference,party_id,payment_date,amount,cash_account_code\nH-SPAY,H-BILL,{party},2020-01-10,60,1100\n"
    );
    let empty = preview(&app, &cookie, "supplier_payments", &payments).await;
    assert_eq!(empty["preview"]["can_commit"], false);
    let fund=preview(&app,&cookie,"journals","reference,posting_date,description,debit_account,credit_account,amount\nH-FUND,2020-01-02,Modal kas lama,1100,3000,200\n").await;
    assert_eq!(fund["preview"]["error_count"], 0);
    commit(&app, &cookie, &fund).await;
    let over = format!(
        "reference,document_reference,party_id,payment_date,amount,cash_account_code\nH-SPAY-1,H-BILL,{party},2020-01-10,60,1100\nH-SPAY-2,H-BILL,{party},2020-01-11,50,1100\n"
    );
    let rejected = preview(&app, &cookie, "supplier_payments", &over).await;
    assert_eq!(rejected["preview"]["error_count"], 1);
    let p = preview(&app, &cookie, "supplier_payments", &payments).await;
    assert_eq!(p["preview"]["error_count"], 0);
    commit(&app, &cookie, &p).await;
    let ap:Decimal=sqlx::query_scalar("SELECT sum(credit-debit) FROM journal_lines WHERE account_code=(SELECT code FROM accounts WHERE system_role='purchase_payable')").fetch_one(&pool).await.unwrap();
    assert_eq!(ap, d("40"));
    let cash: Decimal =
        sqlx::query_scalar("SELECT sum(debit-credit) FROM journal_lines WHERE account_code='1100'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cash, d("140"));
    let earlier = format!(
        "reference,document_reference,party_id,payment_date,amount,cash_account_code\nTOO-EARLY,H-BILL,{party},2020-01-01,1,1100\n"
    );
    assert_eq!(
        preview(&app, &cookie, "supplier_payments", &earlier).await["preview"]["can_commit"],
        false
    );
}
