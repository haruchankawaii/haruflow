use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const REPORT: &str = "/api/accounting/reports";
async fn actor(pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users(id,name,email,password_hash,role) VALUES($1,$2,$3,'unused-fixture',$4)",
    )
    .bind(id)
    .bind(format!("Report {role}"))
    .bind(format!("{id}@example.test"))
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
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
        "INSERT INTO company(id,name,timezone) VALUES($1,'Financial report test','Asia/Jakarta')",
    )
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
    let (user, cookie) = actor(pool, "finance").await;
    (
        crate::app_router(
            crate::AppState {
                db: pool.clone(),
                setup_token: "financial-report-token-at-least-32-characters".into(),
                secure_cookies: false,
            },
            ".local/no-report-static".into(),
        ),
        user,
        cookie,
    )
}
async fn request(
    app: &Router,
    cookie: Option<&str>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-haruflow-request", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn report(app: &Router, cookie: &str, from: &str, to: &str) -> Value {
    let (status, value) = request(
        app,
        Some(cookie),
        "GET",
        &format!("{REPORT}?from={from}&to={to}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    value
}
fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn money(value: &Value, key: &str) -> Decimal {
    decimal(value[key].as_str().unwrap())
}
async fn journal(
    pool: &PgPool,
    user: Uuid,
    source: &str,
    at: &str,
    lines: &[(&str, &str, &str)],
) -> Uuid {
    let mut tx = pool.begin().await.unwrap();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) VALUES($1,'Financial report fixture',$2,$3,$4,$5::text::timestamptz)")
        .bind(id).bind(source).bind(Uuid::new_v4()).bind(user).bind(at).execute(&mut *tx).await.unwrap();
    for (account, debit, credit) in lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(id).bind(account).bind(decimal(debit)).bind(decimal(credit)).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    id
}
fn category<'a>(value: &'a Value, key: &str) -> &'a Value {
    value["cash_flow"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == key)
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_auth_strict_calendar_defaults_and_empty_balances(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    assert_eq!(request(&app, None, "GET", REPORT, None).await.0, 401);
    for query in [
        "from=2026-02-29",
        "from=2026-2-01",
        "to=0000-01-01",
        "to=2026-04-31",
        "from=2026-02-02&to=2026-02-01",
        "from=2026-01-01%00",
        "unknown=1",
    ] {
        assert_eq!(
            request(
                &app,
                Some(&cookie),
                "GET",
                &format!("{REPORT}?{query}"),
                None
            )
            .await
            .0,
            400,
            "{query}"
        );
    }
    let (status, data) = request(&app, Some(&cookie), "GET", REPORT, None).await;
    assert_eq!(status, 200, "{data}");
    let today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date::text FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(data["to"], today);
    assert_eq!(data["from"], format!("{}-01", &today[..7]));
    assert_eq!(data["timezone"], "Asia/Jakarta");
    assert_eq!(data["balance_sheet"]["balanced"], true);
    assert_eq!(data["cash_flow"]["reconciled"], true);
    assert_eq!(
        money(&data["income_statement"], "net_profit"),
        Decimal::ZERO
    );
    assert_eq!(data["cash_flow"]["entries"], json!([]));
    assert_eq!(
        report(&app, &cookie, "0001-01-01", "9999-12-31").await["to"],
        "9999-12-31"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_separate_period_profit_from_cumulative_equity_and_custom_accounts(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES('4100','Jasa kustom','revenue'),('5100','Beban kustom','expense'),('1500','Peralatan','asset'),('2400','Pinjaman','liability')").execute(&pool).await.unwrap();
    journal(
        &pool,
        user,
        "opening",
        "2025-12-31T00:00:00Z",
        &[("1100", "1000", "0"), ("3000", "0", "1000")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-10T00:00:00Z",
        &[("1100", "200", "0"), ("4100", "0", "200")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-20T00:00:00Z",
        &[("5100", "50", "0"), ("1100", "0", "50")],
    )
    .await;
    journal(
        &pool,
        user,
        "invoice",
        "2026-02-10T00:00:00Z",
        &[("1200", "100", "0"), ("4100", "0", "100")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-02-11T00:00:00Z",
        &[("5100", "20", "0"), ("1100", "0", "20")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-02-12T00:00:00Z",
        &[("1100", "40", "0"), ("2400", "0", "40")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-02-13T00:00:00Z",
        &[("1500", "30", "0"), ("1100", "0", "30")],
    )
    .await;
    let data = report(&app, &cookie, "2026-02-01", "2026-02-28").await;
    let income = &data["income_statement"];
    assert_eq!(money(income, "revenue_total"), decimal("100"));
    assert_eq!(money(income, "expense_total"), decimal("20"));
    assert_eq!(money(income, "net_profit"), decimal("80"));
    let balance = &data["balance_sheet"];
    assert_eq!(money(balance, "retained_earnings"), decimal("230"));
    assert_eq!(money(balance, "equity_total"), decimal("1000"));
    assert_eq!(money(balance, "assets_total"), decimal("1270"));
    assert_eq!(money(balance, "liabilities_total"), decimal("40"));
    assert_eq!(
        money(balance, "liabilities_and_equity_total"),
        decimal("1270")
    );
    assert_eq!(balance["balanced"], true);
    assert!(
        balance["asset_accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["code"] == "1500" && money(a, "amount") == decimal("30"))
    );
    let cash = &data["cash_flow"];
    assert_eq!(money(cash, "beginning_balance"), decimal("1150"));
    assert_eq!(money(cash, "ending_balance"), decimal("1140"));
    assert_eq!(money(cash, "cash_in"), decimal("40"));
    assert_eq!(money(cash, "cash_out"), decimal("50"));
    assert_eq!(cash["reconciled"], true);
    let cumulative = report(&app, &cookie, "2025-01-01", "2026-02-28").await;
    assert_eq!(cumulative["balance_sheet"], data["balance_sheet"]);
    assert_eq!(
        money(&cumulative["income_statement"], "net_profit"),
        decimal("230")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_include_inclusive_local_days_and_exclude_adjacent_utc_dates(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    for (time, amount) in [
        ("2026-01-31T16:59:59Z", "1"),
        ("2026-01-31T17:00:00Z", "2"),
        ("2026-02-01T16:59:59.999999Z", "4"),
        ("2026-02-01T17:00:00Z", "8"),
    ] {
        journal(
            &pool,
            user,
            "manual",
            time,
            &[("1100", amount, "0"), ("4000", "0", amount)],
        )
        .await;
    }
    let data = report(&app, &cookie, "2026-02-01", "2026-02-01").await;
    assert_eq!(
        money(&data["income_statement"], "revenue_total"),
        decimal("6")
    );
    assert_eq!(money(&data["cash_flow"], "beginning_balance"), decimal("1"));
    assert_eq!(money(&data["cash_flow"], "ending_balance"), decimal("7"));
    assert_eq!(data["cash_flow"]["entries"].as_array().unwrap().len(), 2);
    assert_eq!(data["balance_sheet"]["retained_earnings"], "7.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_signed_reversals_closed_periods_and_negative_results(pool: PgPool) {
    let (app, user, cookie) = fixture(&pool).await;
    journal(
        &pool,
        user,
        "invoice",
        "2026-01-01T00:00:00Z",
        &[("1100", "100", "0"), ("4000", "0", "100")],
    )
    .await;
    journal(
        &pool,
        user,
        "reversal",
        "2026-02-01T00:00:00Z",
        &[("4000", "120", "0"), ("1100", "0", "120")],
    )
    .await;
    journal(
        &pool,
        user,
        "reversal",
        "2026-02-02T00:00:00Z",
        &[("1100", "40", "0"), ("5000", "0", "40")],
    )
    .await;
    sqlx::query("UPDATE accounting_period SET closed_through='2026-02-28'")
        .execute(&pool)
        .await
        .unwrap();
    let data = report(&app, &cookie, "2026-02-01", "2026-02-28").await;
    assert_eq!(data["income_statement"]["revenue_total"], "-120.00");
    assert_eq!(data["income_statement"]["expense_total"], "-40.00");
    assert_eq!(data["income_statement"]["net_profit"], "-80.00");
    assert_eq!(data["balance_sheet"]["retained_earnings"], "20.00");
    assert_eq!(data["balance_sheet"]["balanced"], true);
    assert_eq!(data["cash_flow"]["net_change"], "-80.00");
    assert_eq!(data["cash_flow"]["ending_balance"], "20.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_cash_classification_keeps_mixed_opening_and_custom_sources_reconciled(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES('1500','Peralatan','asset')")
        .execute(&pool)
        .await
        .unwrap();
    journal(
        &pool,
        user,
        "opening",
        "2026-01-01T00:00:00Z",
        &[("1100", "500", "0"), ("3000", "0", "500")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-02T00:00:00Z",
        &[("1100", "100", "0"), ("4000", "0", "100")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-03T00:00:00Z",
        &[("3000", "40", "0"), ("1100", "0", "40")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-04T00:00:00Z",
        &[
            ("5000", "30", "0"),
            ("1500", "30", "0"),
            ("1100", "0", "60"),
        ],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        "2026-01-05T00:00:00Z",
        &[("1100", "7", "0"), ("1100", "0", "2"), ("4000", "0", "5")],
    )
    .await;
    let payable: String =
        sqlx::query_scalar("SELECT code FROM accounts WHERE system_role='purchase_payable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    journal(
        &pool,
        user,
        "supplier_payment",
        "2026-01-06T00:00:00Z",
        &[(&payable, "10", "0"), ("1100", "0", "10")],
    )
    .await;
    let data = report(&app, &cookie, "2026-01-01", "2026-01-31").await;
    assert_eq!(data["cash_flow"]["cash_in"], "607.00");
    assert_eq!(data["cash_flow"]["cash_out"], "112.00");
    assert_eq!(data["cash_flow"]["ending_balance"], "495.00");
    assert_eq!(data["cash_flow"]["reconciled"], true);
    assert_eq!(category(&data, "operating")["cash_in"], "100.00");
    assert_eq!(category(&data, "operating")["cash_out"], "10.00");
    assert_eq!(category(&data, "financing")["cash_out"], "40.00");
    assert_eq!(category(&data, "unclassified")["cash_in"], "507.00");
    assert_eq!(category(&data, "unclassified")["cash_out"], "62.00");
    assert_eq!(category(&data, "investing")["net_change"], "0.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_classification_permissions_versions_retries_history_and_reset(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    let (_, viewer) = actor(&pool, "viewer").await;
    sqlx::query("INSERT INTO accounts(code,name,kind) VALUES('1500','Peralatan','asset')")
        .execute(&pool)
        .await
        .unwrap();
    let id = journal(
        &pool,
        user,
        "manual",
        "2026-01-01T00:00:00Z",
        &[("1500", "80", "0"), ("1100", "0", "80")],
    )
    .await;
    let noncash = journal(
        &pool,
        user,
        "manual",
        "2026-01-02T00:00:00Z",
        &[("1200", "10", "0"), ("4000", "0", "10")],
    )
    .await;
    sqlx::query("UPDATE accounting_period SET closed_through='2026-01-31'")
        .execute(&pool)
        .await
        .unwrap();
    let path = format!("{REPORT}/cash-flow/{id}");
    let body = json!({"category":"investing","version":0,"reason":"Pembelian peralatan jangka panjang","idempotency_key":Uuid::new_v4()});
    assert_eq!(
        request(&app, None, "PUT", &path, Some(body.clone()))
            .await
            .0,
        401
    );
    assert_eq!(
        request(&app, Some(&viewer), "PUT", &path, Some(body.clone()))
            .await
            .0,
        403
    );
    let (a, b) = tokio::join!(
        request(&app, Some(&cookie), "PUT", &path, Some(body.clone())),
        request(&app, Some(&cookie), "PUT", &path, Some(body.clone()))
    );
    assert_eq!(a.0, 200, "{}", a.1);
    assert_eq!(a, b);
    assert_eq!(a.1["version"], 1);
    let (_, another_finance) = actor(&pool, "finance").await;
    assert_eq!(
        request(
            &app,
            Some(&another_finance),
            "PUT",
            &path,
            Some(body.clone())
        )
        .await
        .0,
        409
    );
    let data = report(&app, &cookie, "2026-01-01", "2026-01-31").await;
    assert_eq!(category(&data, "investing")["cash_out"], "80.00");
    let mut noncash_body = body.clone();
    noncash_body["idempotency_key"] = json!(Uuid::new_v4());
    assert_eq!(
        data["cash_flow"]["entries"][0]["classification"]["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut stale = body.clone();
    stale["idempotency_key"] = json!(Uuid::new_v4());
    assert_eq!(
        request(&app, Some(&cookie), "PUT", &path, Some(stale))
            .await
            .0,
        409
    );
    for (field, value) in [
        ("category", json!("unknown")),
        ("reason", json!("bad\0reason")),
        ("reason", json!("x")),
        ("version", json!(-1)),
    ] {
        let mut invalid = body.clone();
        invalid[field] = value;
        assert_eq!(
            request(&app, Some(&cookie), "PUT", &path, Some(invalid))
                .await
                .0,
            400
        );
    }
    assert_eq!(
        request(
            &app,
            Some(&cookie),
            "PUT",
            &format!("{REPORT}/cash-flow/{noncash}"),
            Some(noncash_body)
        )
        .await
        .0,
        404
    );
    let reset = json!({"category":null,"version":1,"reason":"Kembali ke klasifikasi otomatis","idempotency_key":Uuid::new_v4()});
    let (status, result) = request(&app, Some(&cookie), "PUT", &path, Some(reset)).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["version"], 2);
    let data = report(&app, &cookie, "2026-01-01", "2026-01-31").await;
    assert_eq!(category(&data, "unclassified")["cash_out"], "80.00");
    assert_eq!(
        data["cash_flow"]["entries"][0]["classification"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(data["cash_flow"]["reconciled"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert!(
        sqlx::query("UPDATE cash_flow_classification_events SET reason='rewrite history'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn financial_reports_totals_and_sources_include_more_than_document_list_limit(pool: PgPool) {
    let (app, user, cookie) = fixture(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let ids:Vec<Uuid>=sqlx::query_scalar("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) SELECT gen_random_uuid(),'Many cash sources','manual',gen_random_uuid(),$1,'2026-01-01T00:00:00Z'::timestamptz FROM generate_series(1,510) RETURNING id")
        .bind(user).fetch_all(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),x,'1100',0.01,0 FROM unnest($1::uuid[]) x UNION ALL SELECT gen_random_uuid(),x,'4000',0,0.01 FROM unnest($1::uuid[]) x")
        .bind(&ids).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let data = report(&app, &cookie, "2026-01-01", "2026-01-31").await;
    assert_eq!(data["cash_flow"]["entries"].as_array().unwrap().len(), 510);
    assert_eq!(data["cash_flow"]["cash_in"], "5.10");
    assert_eq!(data["income_statement"]["revenue_total"], "5.10");
    assert_eq!(data["balance_sheet"]["balanced"], true);
}
