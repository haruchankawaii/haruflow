use crate::supplier_bill_tests::{actor, call, fixture, request};
use axum::http::StatusCode;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_cash(pool: &PgPool, user: Uuid, amount: i64) {
    let mut tx = pool.begin().await.unwrap();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Legacy cash','manual',$2,$3)").bind(id).bind(Uuid::new_v4()).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1100',$3,0),($4,$2,'3000',0,$3)").bind(Uuid::new_v4()).bind(id).bind(Decimal::from(amount)).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
async fn balance(pool: &PgPool, code: &str) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code=$1",
    )
    .bind(code)
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn bank(app: &axum::Router, cookie: &str) {
    call(app,cookie,"POST","/api/treasury/accounts",Some(json!({"account_code":"1110","name":"Bank operasional","kind":"bank","bank_name":"Bank uji","account_number":"123456","idempotency_key":Uuid::new_v4()}))).await;
}
fn transfer(value: &str) -> Value {
    json!({"from_account":"1100","to_account":"1110","amount":value,"reason":"Transfer dana nyata","reference":"BANK-T","idempotency_key":Uuid::new_v4()})
}

#[sqlx::test(migrations = "../../migrations")]
async fn transfers_serialize_sufficiency_idempotency_and_original_reversal_without_external_cash_flow(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    seed_cash(&pool, user, 1000).await;
    bank(&app, &cookie).await;
    let original_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_lines")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(original_rows, 2);
    let body = transfer("700");
    let original = call(
        &app,
        &cookie,
        "POST",
        "/api/treasury/transfers",
        Some(body.clone()),
    )
    .await;
    let retry = call(&app, &cookie, "POST", "/api/treasury/transfers", Some(body)).await;
    assert_eq!(original, retry);
    assert_eq!(balance(&pool, "1100").await, Decimal::from(300));
    assert_eq!(balance(&pool, "1110").await, Decimal::from(700));
    let a = request(
        &app,
        &cookie,
        "POST",
        "/api/treasury/transfers",
        Some(transfer("250")),
    );
    let b = request(
        &app,
        &cookie,
        "POST",
        "/api/treasury/transfers",
        Some(transfer("250")),
    );
    let (a, b) = tokio::join!(a, b);
    assert_eq!(
        usize::from(a.0.is_success()) + usize::from(b.0.is_success()),
        1
    );
    assert!(a.0 == StatusCode::CONFLICT || b.0 == StatusCode::CONFLICT);
    assert_eq!(balance(&pool, "1100").await, Decimal::from(50));
    let reverse = json!({"version":original["version"],"reason":"Koreksi transfer pertama","idempotency_key":Uuid::new_v4()});
    let reversed = call(
        &app,
        &cookie,
        "POST",
        &format!(
            "/api/treasury/transfers/{}/reverse",
            original["id"].as_str().unwrap()
        ),
        Some(reverse),
    )
    .await;
    assert_eq!(reversed["version"], 2);
    assert_eq!(balance(&pool, "1100").await, Decimal::from(750));
    assert_eq!(balance(&pool, "1110").await, Decimal::from(250));
    let report = call(&app, &cookie, "GET", "/api/accounting/reports", None).await;
    assert_eq!(report["cash_flow"]["cash_in"], "1000.00");
    assert_eq!(report["cash_flow"]["cash_out"], "0.00");
    assert_eq!(report["cash_flow"]["ending_balance"], "1000.00");
    assert_eq!(report["cash_flow"]["reconciled"], true);
    let chart = call(&app, &cookie, "GET", "/api/dashboard/insights", None).await;
    assert_eq!(
        chart["totals"]["cash_in"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::from(1000)
    );
    assert_eq!(
        chart["totals"]["cash_out"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::ZERO
    );
    let (_, sales) = actor(&pool, "sales").await;
    let denied = request(
        &app,
        &sales,
        "POST",
        "/api/treasury/transfers",
        Some(transfer("1")),
    )
    .await;
    assert_eq!(denied.0, StatusCode::FORBIDDEN);
    sqlx::query(
        "UPDATE accounting_period SET closed_through=(now() AT TIME ZONE 'Asia/Jakarta')::date",
    )
    .execute(&pool)
    .await
    .unwrap();
    let closed = request(
        &app,
        &cookie,
        "POST",
        "/api/treasury/transfers",
        Some(transfer("1")),
    )
    .await;
    assert_eq!(closed.0, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn selected_bank_advances_and_refunds_reverse_to_source_and_preserve_legacy_cash(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    seed_cash(&pool, user, 1000).await;
    bank(&app, &cookie).await;
    let legacy:String=sqlx::query_scalar("SELECT md5(string_agg(to_jsonb(l)::text,'' ORDER BY l.id)) FROM journal_lines l WHERE account_code='1100'").fetch_one(&pool).await.unwrap();
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Pelanggan rekening"})),
    )
    .await;
    let fund=call(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":customer["id"],"cash_account_code":"1110","amount":"500","reference":"BANK-IN","idempotency_key":Uuid::new_v4()}))).await;
    let refunded=call(&app,&cookie,"POST",&format!("/api/party-funds/{}/refunds",fund["id"].as_str().unwrap()),Some(json!({"version":fund["version"],"cash_account_code":"1110","amount":"200","reason":"Refund ke rekening asal","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(balance(&pool, "1110").await, Decimal::from(300));
    let restored=call(&app,&cookie,"POST",&format!("/api/party-refunds/{}/reverse",refunded["refunds"][0]["id"].as_str().unwrap()),Some(json!({"version":refunded["version"],"reason":"Refund dibatalkan bank","idempotency_key":Uuid::new_v4()}))).await;
    call(&app,&cookie,"POST",&format!("/api/party-funds/{}/reverse",fund["id"].as_str().unwrap()),Some(json!({"version":restored["version"],"reason":"Uang muka dikoreksi","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(balance(&pool, "1110").await, Decimal::ZERO);
    assert_eq!(balance(&pool, "1100").await, Decimal::from(1000));
    let after:String=sqlx::query_scalar("SELECT md5(string_agg(to_jsonb(l)::text,'' ORDER BY l.id)) FROM journal_lines l WHERE account_code='1100'").fetch_one(&pool).await.unwrap();
    assert_eq!(legacy, after);
    let bad=request(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":customer["id"],"cash_account_code":"1200","amount":"1","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(bad.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_preview_import_explicit_match_and_audited_unmatch_never_double_post_cash(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    seed_cash(&pool, user, 1000).await;
    bank(&app, &cookie).await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/treasury/transfers",
        Some(transfer("700")),
    )
    .await;
    let today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date::text FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    let csv = format!(
        "date,reference,description,amount\n{today},BANK-001,\"Setoran, rekening\",700.00\n"
    );
    let body = json!({"account_code":"1110","reference":"REKENING-001","from":today,"to":today,"opening_balance":"0","closing_balance":"700","csv":csv});
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    let preview = call(
        &app,
        &cookie,
        "POST",
        "/api/treasury/statements/preview",
        Some(body.clone()),
    )
    .await;
    assert_eq!(preview["count"], 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM bank_statements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let mut import = body.clone();
    import["sha256"] = preview["sha256"].clone();
    import["idempotency_key"] = json!(Uuid::new_v4());
    let imported = call(
        &app,
        &cookie,
        "POST",
        "/api/treasury/statements/import",
        Some(import.clone()),
    )
    .await;
    assert_eq!(
        imported,
        call(
            &app,
            &cookie,
            "POST",
            "/api/treasury/statements/import",
            Some(import.clone())
        )
        .await
    );
    import["idempotency_key"] = json!(Uuid::new_v4());
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/treasury/statements/import",
            Some(import)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let first = call(
        &app,
        &cookie,
        "GET",
        "/api/treasury/reconciliation?account_code=1110",
        None,
    )
    .await;
    assert_eq!(first["statements"][0]["unmatched_bank"], 1);
    let line = &first["lines"][0];
    let target = &first["ledger"][0];
    let wrong: Uuid =
        sqlx::query_scalar("SELECT id FROM journal_lines WHERE account_code='1100' AND credit=700")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mismatch=request(&app,&cookie,"POST",&format!("/api/treasury/statement-lines/{}/match",line["id"].as_str().unwrap()),Some(json!({"version":line["version"],"journal_line_id":wrong,"reason":"Akun sumber berbeda","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(mismatch.0, StatusCode::CONFLICT);
    let path = format!(
        "/api/treasury/statement-lines/{}/match",
        line["id"].as_str().unwrap()
    );
    let matched=call(&app,&cookie,"POST",&path,Some(json!({"version":line["version"],"journal_line_id":target["id"],"reason":"Referensi setoran bank cocok","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(matched["version"], 2);
    assert_eq!(request(&app,&cookie,"POST",&path,Some(json!({"version":1,"journal_line_id":target["id"],"reason":"Percobaan versi lama","idempotency_key":Uuid::new_v4()}))).await.0,StatusCode::CONFLICT);
    let reconciled = call(
        &app,
        &cookie,
        "GET",
        "/api/treasury/reconciliation?account_code=1110",
        None,
    )
    .await;
    assert_eq!(reconciled["statements"][0]["unmatched_bank"], 0);
    assert_eq!(reconciled["statements"][0]["unmatched_ledger"], 0);
    assert_eq!(
        reconciled["statements"][0]["ledger_closing"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::from(700)
    );
    call(&app,&cookie,"POST",&format!("/api/treasury/statement-lines/{}/unmatch",line["id"].as_str().unwrap()),Some(json!({"version":2,"reason":"Periksa ulang referensi bank","idempotency_key":Uuid::new_v4()}))).await;
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM bank_reconciliation_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 2);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(balance(&pool, "1110").await, Decimal::from(700));
    let mut bad = body.clone();
    bad["closing_balance"] = json!("701");
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/treasury/statements/preview",
            Some(bad)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
