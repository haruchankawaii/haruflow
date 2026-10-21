use crate::supplier_bill_tests::{actor, call, fixture, request};
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn dated(
    app: &axum::Router,
    cookie: &str,
    date: &str,
    debit: &str,
    credit: &str,
    amount: &str,
) -> Value {
    call(app,cookie,"POST","/api/accounting/dated-journals",Some(json!({"posting_date":date,"description":"Fiscal dated fixture","reason":"Historical bookkeeping test","confirmed":true,"lines":[{"account_code":debit,"debit":amount,"credit":"0"},{"account_code":credit,"debit":"0","credit":amount}],"idempotency_key":Uuid::new_v4()}))).await
}
fn closing(plan: &Value) -> Value {
    json!({"year":plan["year"],"period_version":plan["period_version"],"journal_count":plan["journal_count"],"reason":"Reviewed annual financial statements","confirmed":true,"idempotency_key":Uuid::new_v4()})
}
#[sqlx::test(migrations = "../../migrations")]
async fn fiscal_close_preserves_profit_balance_sheet_and_original_history_then_reverses(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    dated(&app, &cookie, "2020-03-01", "1100", "4000", "1000").await;
    dated(&app, &cookie, "2020-06-01", "5000", "1100", "200").await;
    let before: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id,md5(to_jsonb(j)::text) FROM journal_entries j ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let plan = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/fiscal/preview?year=2020",
        None,
    )
    .await;
    assert_eq!(plan["net_profit"], "800.00");
    let body = closing(&plan);
    let close = call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/fiscal/close",
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            "/api/accounting/fiscal/close",
            Some(body)
        )
        .await,
        close
    );
    let report = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/reports?from=2020-01-01&to=2020-12-31",
        None,
    )
    .await;
    assert_eq!(report["income_statement"]["net_profit"], "800.00");
    assert_eq!(report["balance_sheet"]["equity_total"], "800.00");
    assert_eq!(report["balance_sheet"]["retained_earnings"], "0.00");
    assert_eq!(report["balance_sheet"]["balanced"], true);
    let dashboard = call(&app, &cookie, "GET", "/api/dashboard", None).await;
    assert_eq!(dashboard["revenue"], "1000.00");
    let state = call(&app, &cookie, "GET", "/api/accounting/fiscal", None).await;
    let reopened=request(&app,&cookie,"POST","/api/accounting/period/reopen",Some(json!({"closed_through":null,"version":state["version"],"reason":"Must reverse closing first","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(reopened.0, StatusCode::CONFLICT);
    let reverse = json!({"period_version":state["version"],"reason":"Correct the annual closing","confirmed":true,"idempotency_key":Uuid::new_v4()});
    let path = format!(
        "/api/accounting/fiscal/{}/reverse",
        close["id"].as_str().unwrap()
    );
    let reversed = call(&app, &cookie, "POST", &path, Some(reverse.clone())).await;
    assert_eq!(
        call(&app, &cookie, "POST", &path, Some(reverse)).await,
        reversed
    );
    let report = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/reports?from=2020-01-01&to=2020-12-31",
        None,
    )
    .await;
    assert_eq!(report["income_statement"]["net_profit"], "800.00");
    assert_eq!(report["balance_sheet"]["equity_total"], "0.00");
    assert_eq!(report["balance_sheet"]["retained_earnings"], "800.00");
    assert_eq!(report["balance_sheet"]["balanced"], true);
    for (id, hash) in before {
        let after: String =
            sqlx::query_scalar("SELECT md5(to_jsonb(j)::text) FROM journal_entries j WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, hash);
    }
    let plan = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/fiscal/preview?year=2020",
        None,
    )
    .await;
    assert_eq!(plan["net_profit"], "800.00");
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/fiscal/close",
        Some(closing(&plan)),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM period_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        3
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn fiscal_versions_reopening_roles_and_dated_journals_cannot_bypass_closed_period(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    dated(&app, &cookie, "2021-03-01", "1100", "4000", "100").await;
    let plan = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/fiscal/preview?year=2021",
        None,
    )
    .await;
    dated(&app, &cookie, "2021-03-02", "1100", "4000", "20").await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/fiscal/close",
            Some(closing(&plan))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    call(&app,&cookie,"POST","/api/accounting/period/close",Some(json!({"through":"2021-12-31","reason":"Lock historical year","idempotency_key":Uuid::new_v4()}))).await;
    let state = call(&app, &cookie, "GET", "/api/accounting/fiscal", None).await;
    assert_eq!(state["version"], 2);
    assert_eq!(state["events"].as_array().unwrap().len(), 1);
    let body = json!({"posting_date":"2021-05-01","description":"Blocked journal","reason":"Closed date test","confirmed":true,"lines":[{"account_code":"1100","debit":"5","credit":"0"},{"account_code":"4000","debit":"0","credit":"5"}],"idempotency_key":Uuid::new_v4()});
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/dated-journals",
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, finance) = actor(&pool, "finance").await;
    let reopen = json!({"closed_through":"2020-12-31","version":state["version"],"reason":"Reviewed correction access","confirmed":true,"idempotency_key":Uuid::new_v4()});
    assert_eq!(
        request(
            &app,
            &finance,
            "POST",
            "/api/accounting/period/reopen",
            Some(reopen.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/period/reopen",
        Some(reopen),
    )
    .await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/dated-journals",
        Some(body),
    )
    .await;
    let inserted:String=sqlx::query_scalar("SELECT (created_at AT TIME ZONE 'Asia/Jakarta')::date::text FROM journal_entries WHERE description='Blocked journal'").fetch_one(&pool).await.unwrap();
    assert_eq!(inserted, "2021-05-01");
    let actual: bool = sqlx::query_scalar(
        "SELECT posted_at>created_at FROM journal_entries WHERE description='Blocked journal'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(actual);
    for date in ["2999-01-01", "2021-2-1", "2021-02-29", "0000-01-01"] {
        let mut bad = json!({"posting_date":date,"description":"Invalid dated journal","reason":"Date validation test","confirmed":true,"lines":[{"account_code":"1100","debit":"1","credit":"0"},{"account_code":"4000","debit":"0","credit":"1"}],"idempotency_key":Uuid::new_v4()});
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                "/api/accounting/dated-journals",
                Some(bad.clone())
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        bad["lines"][0]["account_code"] = json!("1200");
        bad["posting_date"] = json!("2022-01-01");
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                "/api/accounting/dated-journals",
                Some(bad)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap(),
        3
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn fiscal_concurrent_closing_posts_once_and_stale_reverse_is_rejected(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    dated(&app, &cookie, "2022-01-01", "1100", "4000", "90").await;
    let plan = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/fiscal/preview?year=2022",
        None,
    )
    .await;
    let a = closing(&plan);
    let b = closing(&plan);
    let (x, y) = tokio::join!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/fiscal/close",
            Some(a)
        ),
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/fiscal/close",
            Some(b)
        )
    );
    assert_eq!([x.0, y.0].iter().filter(|s| s.is_success()).count(), 1);
    assert!([x.0, y.0].contains(&StatusCode::CONFLICT));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fiscal_closings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let close: Uuid = sqlx::query_scalar("SELECT id FROM fiscal_closings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(request(&app,&cookie,"POST",&format!("/api/accounting/fiscal/{close}/reverse"),Some(json!({"period_version":1,"reason":"Stale reversal attempt","confirmed":true,"idempotency_key":Uuid::new_v4()}))).await.0,StatusCode::CONFLICT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fiscal_closing_reversals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
