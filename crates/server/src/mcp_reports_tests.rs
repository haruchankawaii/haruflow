use crate::supplier_bill_tests::{actor, call, fixture, request};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn credential(app: &Router, admin: &str, id: Uuid, cookie: &str, scopes: &[&str]) -> String {
    call(
        app,
        admin,
        "PUT",
        &format!("/api/ai/access/{id}"),
        Some(json!({"enabled":true,"scopes":scopes})),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        "/api/ai/tokens",
        Some(json!({"name":"MCP report test","scopes":scopes})),
    )
    .await["token"]
        .as_str()
        .unwrap()
        .into()
}
async fn bearer(
    app: &Router,
    token: &str,
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
                .header("authorization", format!("Bearer {token}"))
                .header("x-haruflow-request", "1")
                .header("content-type", "application/json")
                .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
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
async fn tool(app: &Router, token: &str, name: &str, args: Value) -> Value {
    let (status, value) = bearer(
        app,
        token,
        "POST",
        "/api/ai/mcp/call",
        Some(json!({"name":name,"arguments":args})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["isError"], false, "{value}");
    let text: Value = serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, value["structuredContent"]);
    text
}

#[sqlx::test(migrations = "../../migrations")]
async fn mcp_tools_enforce_scopes_cannot_write_and_never_accept_session_cookie(pool: PgPool) {
    let (app, admin, cookie) = fixture(&pool).await;
    let (warehouse, wh_cookie) = actor(&pool, "warehouse").await;
    let key = credential(&app, &cookie, warehouse, &wh_cookie, &["inventory"]).await;
    let (_, list) = bearer(&app, &key, "GET", "/api/ai/mcp/tools", None).await;
    let names: Vec<_> = list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["inventory_risks", "evidence"]);
    for t in list["tools"].as_array().unwrap() {
        assert_eq!(t["annotations"]["readOnlyHint"], true);
    }
    assert_eq!(
        request(&app, &cookie, "GET", "/api/ai/mcp/tools", None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        bearer(&app, &key, "GET", "/api/users", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        bearer(
            &app,
            &key,
            "POST",
            "/api/customers",
            Some(json!({"name":"Forbidden"}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for (name, args) in [
        (
            "business_summary",
            json!({"from":"2026-01-01","to":"2026-01-31"}),
        ),
        ("evidence", json!({"kind":"journals"})),
        ("evidence", json!({"kind":"customers"})),
        ("execute_sql", json!({"sql":"SELECT * FROM users"})),
    ] {
        let (_, v) = bearer(
            &app,
            &key,
            "POST",
            "/api/ai/mcp/call",
            Some(json!({"name":name,"arguments":args})),
        )
        .await;
        assert_eq!(v["isError"], true, "{v}");
        assert!(v.get("structuredContent").is_none());
    }
    let _ = tool(&app, &key, "inventory_risks", json!({})).await;
    call(
        &app,
        &cookie,
        "PUT",
        &format!("/api/ai/access/{warehouse}"),
        Some(json!({"enabled":false,"scopes":[]})),
    )
    .await;
    assert_eq!(
        bearer(&app, &key, "GET", "/api/ai/mcp/tools", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let own = credential(
        &app,
        &cookie,
        admin,
        &cookie,
        &["finance", "sales", "inventory"],
    )
    .await;
    let p = bearer(&app, &own, "GET", "/api/ai/mcp/profile", None)
        .await
        .1;
    assert_eq!(p["read_only"], true);
    assert!(p["user"].get("email").is_none());
    let audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action='ai.tool.read'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 5);
    let audit_text: String = sqlx::query_scalar(
        "SELECT details::text FROM audit_events WHERE action='ai.tool.read' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!audit_text.contains(&key));
    assert!(!audit_text.contains("SELECT"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn mcp_finance_matches_full_ledger_and_bounds_cash_detail_without_partial_totals(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let key = credential(&app, &cookie, admin, &cookie, &["finance"]).await;
    let mut tx = pool.begin().await.unwrap();
    let ids:Vec<Uuid>=sqlx::query_scalar("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) SELECT gen_random_uuid(),'MCP fixture untrusted note','manual',gen_random_uuid(),$1,'2026-01-01T00:00:00Z'::timestamptz FROM generate_series(1,105) RETURNING id").bind(admin).fetch_all(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),x,'1100',1,0 FROM unnest($1::uuid[]) x UNION ALL SELECT gen_random_uuid(),x,'4000',0,1 FROM unnest($1::uuid[]) x").bind(&ids).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    let args = json!({"from":"2026-01-01","to":"2026-01-31"});
    let summary = tool(&app, &key, "business_summary", args.clone()).await;
    let full = call(
        &app,
        &cookie,
        "GET",
        "/api/accounting/reports?from=2026-01-01&to=2026-01-31",
        None,
    )
    .await;
    assert_eq!(summary["income_statement"]["net_profit"], "105.00");
    assert_eq!(
        summary["balance_sheet"]["assets_total"],
        full["balance_sheet"]["assets_total"]
    );
    assert_eq!(summary["timezone"], "Asia/Jakarta");
    assert_eq!(summary["currency"], "IDR");
    assert!(summary.get("cash_flow").is_none());
    let cash = tool(&app, &key, "cash_position", args).await;
    assert_eq!(cash["cash_flow"]["cash_in"], "105.00");
    assert_eq!(cash["cash_flow"]["entries"].as_array().unwrap().len(), 100);
    assert_eq!(cash["cash_flow"]["entry_coverage"]["truncated"], true);
    assert_eq!(cash["cash_flow"]["entry_coverage"]["total"], 105);
    assert_eq!(cash["cash_flow"]["reconciled"], true);
    assert!(!cash.to_string().contains("untrusted note"));
    let evidence = tool(
        &app,
        &key,
        "evidence",
        json!({"kind":"journals","page_size":1,"q":"MCP fixture"}),
    )
    .await;
    assert_eq!(evidence["pagination"]["total"], 105);
    assert_eq!(evidence["items"].as_array().unwrap().len(), 1);
    assert!(evidence["metadata"]["as_of"].is_string());
    assert_eq!(evidence["content_is_untrusted_data"], true);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap(),
        before
    );
    for invalid in [
        json!({"from":"2026-02-29","to":"2026-03-01"}),
        json!({"from":"2026-02-01","to":"2026-01-01"}),
        json!({"from":"2020-01-01","to":"2026-01-01"}),
        json!({"from":"2026-01-01","to":"2026-01-31","sql":"anything"}),
        json!([]),
    ] {
        let (_, v) = bearer(
            &app,
            &key,
            "POST",
            "/api/ai/mcp/call",
            Some(json!({"name":"business_summary","arguments":invalid})),
        )
        .await;
        assert_eq!(v["isError"], true, "{v}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn mcp_aging_includes_imports_and_opening_and_keeps_historical_cutoff(pool: PgPool) {
    let (app, admin, cookie) = fixture(&pool).await;
    let key = credential(&app, &cookie, admin, &cookie, &["finance", "sales"]).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"MCP aging"})),
    )
    .await;
    let draft=call(&app,&cookie,"PUT","/api/accounting/opening",Some(json!({"version":0,"description":"MCP opening debt","items":[],"parties":[{"kind":"receivable","party_id":customer["id"],"reference":"MCP-OPEN","document_date":"2020-01-01","due_date":"2020-02-01","amount":"200"}],"lines":[{"account_code":"3000","debit":"0","credit":"200"}]}))).await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/opening/post",
        Some(json!({"version":draft["document"]["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let csv = format!(
        "reference,party_id,document_date,due_date,total,counter_account,description\nMCP-HIST,{},2020-01-02,2020-02-02,100,4000,Imported debt\n",
        customer["id"].as_str().unwrap()
    );
    let preview = call(
        &app,
        &cookie,
        "POST",
        "/api/transaction-imports/customer_invoices/preview",
        Some(json!({"csv":csv,"delimiter":","})),
    )
    .await;
    assert_eq!(preview["preview"]["error_count"], 0);
    call(
        &app,
        &cookie,
        "POST",
        &format!(
            "/api/transaction-imports/{}/commit",
            preview["id"].as_str().unwrap()
        ),
        Some(json!({"confirmed":true,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let current = tool(&app, &key, "receivables_payables", json!({"limit":1})).await;
    assert_eq!(current["customer"]["outstanding"], "300.00");
    assert_eq!(current["customer"]["reconciliation"]["balanced"], true);
    assert_eq!(current["customer"]["coverage"]["total"], 2);
    assert_eq!(current["customer"]["coverage"]["truncated"], true);
    let historical = tool(
        &app,
        &key,
        "receivables_payables",
        json!({"as_of":"2020-03-01"}),
    )
    .await;
    assert_eq!(historical["customer"]["outstanding"], "100.00");
    assert_eq!(
        historical["customer"]["documents"][0]["target_type"],
        "historical_receivable"
    );
    assert_eq!(historical["customer"]["reconciliation"]["balanced"], true);
    let operational = tool(
        &app,
        &key,
        "sales_summary",
        json!({"from":"2020-01-01","to":"2020-01-31"}),
    )
    .await;
    assert_eq!(operational["operational_invoices"]["invoice_gross"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn mcp_inventory_reads_current_positions_without_cost_or_false_reorder_threshold(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let (warehouse, warehouse_cookie) = actor(&pool, "warehouse").await;
    let key = credential(&app, &cookie, warehouse, &warehouse_cookie, &["inventory"]).await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/products",
        Some(json!({"sku":"MCP-GOODS","name":"MCP product","unit":"pcs","price":"100"})),
    )
    .await;
    let result = tool(&app, &key, "inventory_risks", json!({"limit":1})).await;
    assert_eq!(result["goods_without_available_stock"], 1);
    assert!(result.get("minimum_stock").is_none());
    assert!(!result.to_string().contains("average_cost"));
    assert!(!result.to_string().contains("inventory_value"));
    let evidence = tool(
        &app,
        &key,
        "evidence",
        json!({"kind":"inventory-movements"}),
    )
    .await;
    assert_eq!(evidence["pagination"]["total"], 0);
    for invalid in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":-1}),
        json!({"as_of":"2020-01-01"}),
    ] {
        let (_, v) = bearer(
            &app,
            &key,
            "POST",
            "/api/ai/mcp/call",
            Some(json!({"name":"inventory_risks","arguments":invalid})),
        )
        .await;
        assert_eq!(v["isError"], true, "{v}");
    }
}
