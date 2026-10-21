use axum::{Router, body::Body, http::Request};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::str::FromStr;
use tower::ServiceExt;
use uuid::Uuid;

async fn fixture(pool: &PgPool) -> (Router, Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO company(id,name,timezone) VALUES($1,'Chart test','Asia/Jakarta')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Chart viewer','charts@example.test','unused-fixture','viewer')")
        .bind(id).execute(pool).await.unwrap();
    let token = "e".repeat(64);
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '1 day')",
    )
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    let app = crate::app_router(
        crate::AppState {
            db: pool.clone(),
            setup_token: "chart-test-setup-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        "apps/web/dist".into(),
    );
    (app, id, format!("hf_session={token}"))
}

async fn request(app: &Router, cookie: Option<&str>, path: &str) -> (u16, Value) {
    let mut builder = Request::builder().uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn amount(value: &Value, field: &str) -> Decimal {
    Decimal::from_str(value[field].as_str().unwrap()).unwrap()
}

async fn journal(
    pool: &PgPool,
    user: Uuid,
    source: &str,
    at: DateTime<Utc>,
    lines: &[(&str, &str, &str)],
) {
    let mut tx = pool.begin().await.unwrap();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by,created_at) VALUES($1,'Chart fixture',$2,$3,$4,$5)")
        .bind(id).bind(source).bind(Uuid::new_v4()).bind(user).bind(at).execute(&mut *tx).await.unwrap();
    for (account, debit, credit) in lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(id).bind(account).bind(Decimal::from_str(debit).unwrap())
            .bind(Decimal::from_str(credit).unwrap()).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn insights_are_authenticated_zero_filled_and_validate_the_range(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    assert_eq!(request(&app, None, "/api/dashboard/insights").await.0, 401);
    for invalid in ["0", "1", "2", "4", "13", "255", "abc"] {
        assert_eq!(
            request(
                &app,
                Some(&cookie),
                &format!("/api/dashboard/insights?months={invalid}")
            )
            .await
            .0,
            400
        );
    }
    assert_eq!(
        request(&app, Some(&cookie), "/api/dashboard/insights?unknown=6")
            .await
            .0,
        400
    );
    for months in [3, 6, 12] {
        let (status, data) = request(
            &app,
            Some(&cookie),
            &format!("/api/dashboard/insights?months={months}"),
        )
        .await;
        assert_eq!(status, 200, "{data}");
        assert_eq!(data["months"], months);
        assert_eq!(data["timezone"], "Asia/Jakarta");
        let series = data["series"].as_array().unwrap();
        assert_eq!(series.len(), months as usize);
        assert!(
            series
                .windows(2)
                .all(|pair| pair[0]["month"].as_str() < pair[1]["month"].as_str())
        );
        for point in series {
            for field in [
                "revenue",
                "cogs",
                "gross_profit",
                "cash_in",
                "cash_out",
                "cash_net",
            ] {
                assert_eq!(amount(point, field), Decimal::ZERO);
            }
        }
        assert_eq!(amount(&data["receivables"], "invoiced"), Decimal::ZERO);
        assert_eq!(amount(&data["totals"], "gross_profit"), Decimal::ZERO);
    }
    assert_eq!(
        request(&app, Some(&cookie), "/api/dashboard/insights")
            .await
            .1["months"],
        6
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn insights_use_local_posting_months_and_include_signed_corrections(pool: PgPool) {
    let (app, user, cookie) = fixture(&pool).await;
    let start: DateTime<Utc> = sqlx::query_scalar(
        "SELECT date_trunc('month',now() AT TIME ZONE timezone) AT TIME ZONE timezone FROM company",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let previous: DateTime<Utc> = sqlx::query_scalar("SELECT (date_trunc('month',now() AT TIME ZONE timezone)-interval '1 month') AT TIME ZONE timezone FROM company")
        .fetch_one(&pool).await.unwrap();
    let outside: DateTime<Utc> = sqlx::query_scalar("SELECT (date_trunc('month',now() AT TIME ZONE timezone)-interval '12 months') AT TIME ZONE timezone FROM company")
        .fetch_one(&pool).await.unwrap();
    // At the Indonesian month boundary the UTC calendar still has the preceding month.
    journal(
        &pool,
        user,
        "invoice",
        start,
        &[("1200", "1000", "0"), ("4000", "0", "1000")],
    )
    .await;
    journal(
        &pool,
        user,
        "shipment",
        start,
        &[("5000", "200", "0"), ("1300", "0", "200")],
    )
    .await;
    journal(
        &pool,
        user,
        "reversal",
        start,
        &[("4000", "250", "0"), ("1200", "0", "250")],
    )
    .await;
    journal(
        &pool,
        user,
        "reversal",
        previous,
        &[("4000", "100", "0"), ("1200", "0", "100")],
    )
    .await;
    journal(
        &pool,
        user,
        "opening",
        start,
        &[("1100", "400", "0"), ("3000", "0", "400")],
    )
    .await;
    journal(
        &pool,
        user,
        "manual",
        start,
        &[("3000", "150", "0"), ("1100", "0", "150")],
    )
    .await;
    journal(
        &pool,
        user,
        "invoice",
        outside,
        &[("1200", "999999", "0"), ("4000", "0", "999999")],
    )
    .await;
    journal(
        &pool,
        user,
        "invoice",
        Utc::now() + Duration::days(2),
        &[("1200", "888888", "0"), ("4000", "0", "888888")],
    )
    .await;
    let (status, data) = request(&app, Some(&cookie), "/api/dashboard/insights?months=3").await;
    assert_eq!(status, 200, "{data}");
    let series = data["series"].as_array().unwrap();
    let current = &series[2];
    assert_eq!(amount(current, "revenue"), Decimal::from(750));
    assert_eq!(amount(current, "cogs"), Decimal::from(200));
    assert_eq!(amount(current, "gross_profit"), Decimal::from(550));
    assert_eq!(amount(current, "cash_in"), Decimal::from(400));
    assert_eq!(amount(current, "cash_out"), Decimal::from(150));
    assert_eq!(amount(current, "cash_net"), Decimal::from(250));
    assert_eq!(amount(&series[1], "revenue"), Decimal::from(-100));
    assert_eq!(amount(&series[0], "revenue"), Decimal::ZERO);
    assert_eq!(amount(&data["totals"], "revenue"), Decimal::from(650));
    assert_eq!(amount(&data["totals"], "gross_profit"), Decimal::from(450));
    assert_eq!(amount(&data["totals"], "cash_net"), Decimal::from(250));
    let today: String =
        sqlx::query_scalar("SELECT to_char(now() AT TIME ZONE timezone,'YYYY-MM-DD') FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(data["through"], today);
    let chart_month: String =
        sqlx::query_scalar("SELECT to_char(now() AT TIME ZONE timezone,'YYYY-MM') FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current["month"], chart_month);
}

#[sqlx::test(migrations = "../../migrations")]
async fn insights_receivables_are_the_current_active_position_outside_the_chart_range(
    pool: PgPool,
) {
    let (app, user, cookie) = fixture(&pool).await;
    let customer = Uuid::new_v4();
    sqlx::query("INSERT INTO customers(id,name) VALUES($1,'Chart customer')")
        .bind(customer)
        .execute(&pool)
        .await
        .unwrap();
    for (index, (status, total, paid)) in [
        ("partial", 1000, 300),
        ("unpaid", 200, 0),
        ("paid", 400, 400),
        ("void", 999, 0),
    ]
    .iter()
    .enumerate()
    {
        let order = Uuid::new_v4();
        sqlx::query("INSERT INTO sales_orders(id,number,customer_id,status,total,created_by,created_at) VALUES($1,$2,$3,'invoiced',$4,$5,now()-interval '2 years')")
            .bind(order).bind(format!("CHART-O-{index}")).bind(customer).bind(Decimal::from(*total)).bind(user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO invoices(id,number,order_id,customer_id,status,total,paid,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,now()-interval '2 years')")
            .bind(Uuid::new_v4()).bind(format!("CHART-I-{index}")).bind(order).bind(customer).bind(status)
            .bind(Decimal::from(*total)).bind(Decimal::from(*paid)).execute(&pool).await.unwrap();
    }
    for months in [3, 12] {
        let (status, data) = request(
            &app,
            Some(&cookie),
            &format!("/api/dashboard/insights?months={months}"),
        )
        .await;
        assert_eq!(status, 200, "{data}");
        assert_eq!(
            amount(&data["receivables"], "invoiced"),
            Decimal::from(1600)
        );
        assert_eq!(amount(&data["receivables"], "paid"), Decimal::from(700));
        assert_eq!(
            amount(&data["receivables"], "outstanding"),
            Decimal::from(900)
        );
        assert_eq!(data["receivables"]["unpaid_count"], 1);
        assert_eq!(data["receivables"]["partial_count"], 1);
        assert_eq!(data["receivables"]["paid_count"], 1);
        assert_eq!(amount(&data["totals"], "revenue"), Decimal::ZERO);
    }
    // A different local calendar still produces exactly the requested sequence.
    sqlx::query("UPDATE company SET timezone='America/Los_Angeles'")
        .execute(&pool)
        .await
        .unwrap();
    let data = request(&app, Some(&cookie), "/api/dashboard/insights?months=12")
        .await
        .1;
    assert_eq!(data["timezone"], "America/Los_Angeles");
    let today: String =
        sqlx::query_scalar("SELECT to_char(now() AT TIME ZONE timezone,'YYYY-MM-DD') FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(data["through"], today);
    assert_eq!(data["series"].as_array().unwrap().len(), 12);
    assert_eq!(
        data["receivables"],
        json!({"invoiced":"1600.00","paid":"700.00","outstanding":"900.00","unpaid_count":1,"partial_count":1,"paid_count":1})
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn insights_keep_invoice_settlement_separate_from_opening_debt_and_actual_cash(pool: PgPool) {
    use crate::supplier_bill_tests::{call, fixture};

    let (app, user, cookie) = fixture(&pool).await;
    let customer = call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Opening and operational invoice customer"})),
    )
    .await;
    let party: Uuid = customer["id"].as_str().unwrap().parse().unwrap();
    let draft = call(
        &app,
        &cookie,
        "PUT",
        "/api/accounting/opening",
        Some(json!({
            "version":0,"description":"Legacy receivable","items":[],
            "parties":[{"kind":"receivable","party_id":party,"reference":"OLD-AR-CHART",
                "document_date":"2020-01-01","due_date":"2020-02-01","amount":"2000"}],
            "lines":[{"account_code":"3000","debit":"0","credit":"2000"}]
        })),
    )
    .await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/accounting/opening/post",
        Some(json!({"version":draft["document"]["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let opening: Uuid =
        sqlx::query_scalar("SELECT id FROM opening_party_items WHERE kind='receivable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let invoice = Uuid::new_v4();
    let order = Uuid::new_v4();
    let entry = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO sales_orders(id,number,customer_id,status,total,created_by) VALUES($1,'CHART-OP-SO',$2,'invoiced',1000,$3)")
        .bind(order).bind(party).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO invoices(id,number,order_id,customer_id,total) VALUES($1,'CHART-OP-INV',$2,$3,1000)")
        .bind(invoice).bind(order).bind(party).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,'Operational invoice','invoice',$2,$3)")
        .bind(entry).bind(invoice).bind(user).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,'1200',1000,0),($3,$2,'4000',0,1000)")
        .bind(Uuid::new_v4()).bind(entry).bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let fund = call(
        &app,
        &cookie,
        "POST",
        "/api/party-funds",
        Some(json!({"side":"customer","party_id":party,"amount":"500","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let allocated = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/party-funds/{}/allocate", fund["id"].as_str().unwrap()),
        Some(json!({"version":fund["version"],"items":[
            {"target_type":"invoice","target_id":invoice,"version":1,"amount":"300"},
            {"target_type":"opening_receivable","target_id":opening,"version":1,"amount":"200"}
        ],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let chart = call(&app, &cookie, "GET", "/api/dashboard/insights", None).await;
    assert_eq!(
        amount(&chart["receivables"], "invoiced"),
        Decimal::from(1000)
    );
    assert_eq!(amount(&chart["receivables"], "paid"), Decimal::from(300));
    assert_eq!(
        amount(&chart["receivables"], "outstanding"),
        Decimal::from(700)
    );
    assert_eq!(chart["receivables"]["partial_count"], 1);
    assert_eq!(amount(&chart["totals"], "cash_in"), Decimal::from(500));
    assert_eq!(amount(&chart["totals"], "cash_out"), Decimal::ZERO);
    assert_eq!(amount(&chart["totals"], "revenue"), Decimal::from(1000));
    let allocation = allocated["allocations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|allocation| allocation["target_type"] == "invoice")
        .unwrap();
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/party-allocations/{}/reverse", allocation["id"].as_str().unwrap()),
        Some(json!({"version":allocated["version"],"reason":"Correct invoice allocation","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let corrected = call(&app, &cookie, "GET", "/api/dashboard/insights", None).await;
    assert_eq!(amount(&corrected["receivables"], "paid"), Decimal::ZERO);
    assert_eq!(
        amount(&corrected["receivables"], "outstanding"),
        Decimal::from(1000)
    );
    assert_eq!(corrected["receivables"]["unpaid_count"], 1);
    assert_eq!(
        corrected["totals"], chart["totals"],
        "Allocations and their reversals do not move cash or recognize revenue"
    );
}
