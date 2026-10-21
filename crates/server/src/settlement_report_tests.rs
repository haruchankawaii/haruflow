use crate::supplier_bill_tests::{call, fixture};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
async fn dashboard_receivables_include_unsettled_opening_documents(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Opening dashboard customer"})),
    )
    .await;
    let draft=call(&app,&cookie,"PUT","/api/accounting/opening",Some(json!({"version":0,"description":"Opening receivable dashboard","items":[],"parties":[{"kind":"receivable","party_id":customer["id"],"reference":"OLD-AR-DASHBOARD","document_date":"2020-01-01","due_date":"2020-02-01","amount":"2000000"}],"lines":[{"account_code":"3000","debit":"0","credit":"2000000"}]}))).await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/opening/post",
        Some(json!({"version":draft["document"]["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let target: Uuid =
        sqlx::query_scalar("SELECT id FROM opening_party_items WHERE kind='receivable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let fund=call(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":"customer","party_id":customer["id"],"amount":"500000","idempotency_key":Uuid::new_v4()}))).await;
    call(&app,&cookie,"POST",&format!("/api/party-funds/{}/allocate",fund["id"].as_str().unwrap()),Some(json!({"version":fund["version"],"items":[{"target_type":"opening_receivable","target_id":target,"version":1,"amount":"500000"}],"idempotency_key":Uuid::new_v4()}))).await;
    let dashboard = call(&app, &cookie, "GET", "/api/dashboard", None).await;
    assert_eq!(dashboard["outstanding"], "1500000.00");
    assert_eq!(dashboard["cash"], "500000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn advances_and_actual_refunds_reconcile_balance_sheet_without_recognizing_revenue(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Report customer"})),
    )
    .await;
    let supplier=call(&app,&cookie,"POST","/api/suppliers",Some(json!({"code":"REPORT-SUPPLIER","name":"Report supplier","idempotency_key":Uuid::new_v4()}))).await;
    let mut sources = Vec::new();
    for (side, party, amount) in [
        ("customer", &customer, "1000"),
        ("supplier", &supplier, "300"),
    ] {
        sources.push(call(&app,&cookie,"POST","/api/party-funds",Some(json!({"side":side,"party_id":party["id"],"amount":amount,"idempotency_key":Uuid::new_v4()}))).await);
    }
    for (source, amount) in sources.iter().zip(["200", "100"]) {
        call(&app,&cookie,"POST",&format!("/api/party-funds/{}/refunds",source["id"].as_str().unwrap()),Some(json!({"version":source["version"],"amount":amount,"reason":"Actual refund report check","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    }
    let report = call(&app, &cookie, "GET", "/api/accounting/reports", None).await;
    assert_eq!(report["income_statement"]["revenue_total"], "0.00");
    assert_eq!(report["income_statement"]["expense_total"], "0.00");
    assert_eq!(report["balance_sheet"]["assets_total"], "800.00");
    assert_eq!(report["balance_sheet"]["liabilities_total"], "800.00");
    assert_eq!(report["balance_sheet"]["balanced"], true);
    let flow = &report["cash_flow"];
    assert_eq!(flow["ending_balance"], "600.00");
    assert_eq!(flow["reconciled"], true);
    let operating = flow["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|category| category["key"] == "operating")
        .unwrap();
    assert_eq!(operating["cash_in"], "1100.00");
    assert_eq!(operating["cash_out"], "500.00");
    assert_eq!(flow["entries"].as_array().unwrap().len(), 4);
}
