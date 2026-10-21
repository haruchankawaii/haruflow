use crate::supplier_bill_tests::{
    actor, bill, call, fixture, product, request, snapshot, supplier,
};
use axum::{Router, http::StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap()
}
fn d(v: &str) -> Decimal {
    v.parse().unwrap()
}
fn amount(v: &Value, key: &str) -> Decimal {
    d(v[key].as_str().unwrap())
}
async fn setup(pool: &PgPool) -> (Router, String, Value, Value) {
    let (app, _, cookie) = fixture(pool).await;
    let supplier = supplier(&app, &cookie, "RETURN-SUP").await;
    let product = product(&app, &cookie, "RETURN-ITEM").await;
    let draft=call(&app,&cookie,"POST","/api/purchase-orders",Some(json!({"supplier_id":supplier["id"],"items":[{"product_id":product["id"],"quantity":"3","unit_cost":"10"}],"idempotency_key":Uuid::new_v4()}))).await;
    let confirmed = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/purchase-orders/{}/confirm", id(&draft)),
        Some(json!({"version":draft["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let received = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/purchase-orders/{}/receive", id(&draft)),
        Some(json!({"version":confirmed["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let invoice = bill(pool, &app, &cookie, &received, "RETURN-INVOICE").await;
    (app, cookie, product, invoice)
}
async fn fund(app: &Router, cookie: &str, value: &str) {
    call(app,cookie,"POST","/api/accounting/journals",Some(json!({"description":"Cash fixture for supplier returns","lines":[{"account_code":"1100","debit":value,"credit":"0"},{"account_code":"3000","debit":"0","credit":value}],"idempotency_key":Uuid::new_v4()}))).await;
}
async fn pay(app: &Router, cookie: &str, bill: &Value, value: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-bills/{}/payments", id(bill)),
        Some(json!({"version":bill["version"],"amount":value,"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
fn body(bill: &Value, quantity: &str) -> Value {
    json!({"version":bill["version"],"reason":"Barang rusak dikembalikan ke supplier","items":[{"bill_line_id":bill["items"][0]["id"],"quantity":quantity}],"refund_confirmed":true,"idempotency_key":Uuid::new_v4()})
}
fn path(bill: &Value) -> String {
    format!("/api/purchase-bills/{}/returns", id(bill))
}
async fn stock(pool: &PgPool, product: &Value) -> (Decimal, Decimal, Decimal) {
    sqlx::query_as("SELECT quantity,average_cost,inventory_value FROM products WHERE id=$1")
        .bind(Uuid::parse_str(id(product)).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn cash(pool: &PgPool) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1100'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn assert_ledger(pool: &PgPool) {
    let(stock,inventory):(Decimal,Decimal)=sqlx::query_as("SELECT (SELECT COALESCE(sum(inventory_value),0) FROM products),(SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1300')").fetch_one(pool).await.unwrap();
    assert_eq!(stock, inventory);
    let(due,control):(Decimal,Decimal)=sqlx::query_as("SELECT (SELECT COALESCE(sum(total-credit_total-paid),0) FROM purchase_bills WHERE status<>'void'),(SELECT COALESCE(sum(credit-debit),0) FROM journal_lines WHERE account_code=(SELECT code FROM accounts WHERE system_role='purchase_payable'))").fetch_one(pool).await.unwrap();
    assert_eq!(due, control);
    let unbalanced:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit)) x").fetch_one(pool).await.unwrap();
    assert_eq!(unbalanced, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_deferred_return_retains_receivable_without_cash_and_reverses_after_reservation_cancel(
    pool: PgPool,
) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    fund(&app, &cookie, "100").await;
    let paid = pay(&app, &cookie, &invoice, "20").await;
    let original_stock = stock(&pool, &product).await;
    let mut input = body(&paid, "2");
    input["refund_confirmed"] = json!(false);
    input["refund_mode"] = json!("deferred");
    let returned = call(&app, &cookie, "POST", &path(&paid), Some(input.clone())).await;
    assert_eq!(
        call(&app, &cookie, "POST", &path(&paid), Some(input)).await,
        returned
    );
    assert_eq!(cash(&pool).await, d("80"));
    assert_eq!(amount(&returned, "paid"), d("10"));
    assert_eq!(amount(&returned, "net_total"), d("10"));
    let source = &returned["returns"][0];
    assert_eq!(amount(source, "refund"), Decimal::ZERO);
    assert_eq!(amount(source, "settlement_credit"), d("10"));
    assert_eq!(source["can_reverse"], false);
    let credit = call(
        &app,
        &cookie,
        "GET",
        &format!(
            "/api/party-funds/{}",
            source["credit_fund_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(amount(&credit, "reserved"), d("10"));
    assert_eq!(amount(&credit, "available"), Decimal::ZERO);
    let reverse = format!(
        "/api/supplier-returns/{}/reverse",
        source["id"].as_str().unwrap()
    );
    let reverse_input = json!({"version":returned["version"],"reason":"Perbaiki retur tanpa mengubah kas","idempotency_key":Uuid::new_v4()});
    assert_eq!(
        request(&app, &cookie, "POST", &reverse, Some(reverse_input.clone()))
            .await
            .0,
        409
    );
    call(&app,&cookie,"POST",&format!("/api/party-refund-requests/{}/cancel",source["refund_request_id"].as_str().unwrap()),Some(json!({"version":credit["version"],"reason":"Permintaan refund dibatalkan sebelum retur dikoreksi","idempotency_key":Uuid::new_v4()}))).await;
    let reversed = call(&app, &cookie, "POST", &reverse, Some(reverse_input)).await;
    assert_eq!(amount(&reversed, "paid"), d("20"));
    assert_eq!(amount(&reversed, "credit_total"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, d("80"));
    assert_eq!(stock(&pool, &product).await, original_stock);
    assert_ledger(&pool).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_partial_payment_refund_and_exact_reversal_preserve_original_history(
    pool: PgPool,
) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    fund(&app, &cookie, "100").await;
    let paid = pay(&app, &cookie, &invoice, "20").await;
    let before = snapshot(&pool).await;
    let mut input = body(&paid, "2");
    input["refund_confirmed"] = json!(false);
    assert_eq!(
        request(&app, &cookie, "POST", &path(&paid), Some(input.clone()))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(snapshot(&pool).await, before);
    input["refund_confirmed"] = json!(true);
    let result = call(&app, &cookie, "POST", &path(&paid), Some(input.clone())).await;
    assert_eq!(result["status"], "paid");
    assert_eq!(amount(&result, "total"), d("30"));
    assert_eq!(amount(&result, "credit_total"), d("20"));
    assert_eq!(amount(&result, "paid"), d("10"));
    assert_eq!(amount(&result, "outstanding"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, d("90"));
    let analysis = call(
        &app,
        &cookie,
        "GET",
        "/api/purchase-bills/analysis?months=6",
        None,
    )
    .await;
    let series = analysis["series"].as_array().unwrap();
    assert_eq!(
        series.iter().map(|row| amount(row, "net")).sum::<Decimal>(),
        d("10")
    );
    assert_eq!(
        series
            .iter()
            .map(|row| amount(row, "paid"))
            .sum::<Decimal>(),
        d("20")
    );
    assert_eq!(
        series
            .iter()
            .map(|row| amount(row, "reversed"))
            .sum::<Decimal>(),
        d("10")
    );

    assert_eq!(stock(&pool, &product).await, (d("1"), d("10"), d("10")));
    assert_eq!(
        call(&app, &cookie, "POST", &path(&paid), Some(input)).await,
        result
    );
    assert_eq!(result["payments"][0]["amount"], "20.00");
    assert_eq!(result["payments"][0]["can_reverse"], false);
    assert_eq!(result["can_reverse"], false);
    let source = &result["returns"][0];
    assert_eq!(amount(source, "refund"), d("10"));
    assert_eq!(amount(source, "credit_total"), d("20"));
    assert_eq!(source["can_reverse"], true);
    let recovered=call(&app,&cookie,"POST",&format!("/api/supplier-returns/{}/reverse",id(source)),Some(json!({"version":result["version"],"reason":"Supplier membatalkan persetujuan retur","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(amount(&recovered, "credit_total"), Decimal::ZERO);
    assert_eq!(amount(&recovered, "paid"), d("20"));
    assert_eq!(recovered["status"], "partial");
    assert_eq!(cash(&pool).await, d("80"));
    assert_eq!(stock(&pool, &product).await, (d("3"), d("10"), d("30")));
    assert_eq!(recovered["payments"][0]["can_reverse"], true);
    assert!(recovered["returns"][0]["reversal"].is_object());
    assert_ledger(&pool).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_removes_current_carrying_cost_and_posts_protected_variance(pool: PgPool) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    call(&app,&cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":product["id"],"quantity":"3","unit_cost":"30","idempotency_key":Uuid::new_v4()}))).await;
    let result = call(
        &app,
        &cookie,
        "POST",
        &path(&invoice),
        Some(body(&invoice, "1")),
    )
    .await;
    assert_eq!(amount(&result, "credit_total"), d("10"));
    assert_eq!(amount(&result, "outstanding"), d("20"));
    assert_eq!(stock(&pool, &product).await, (d("5"), d("20"), d("100")));
    let source = &result["returns"][0];
    assert_eq!(amount(source, "inventory_value"), d("20"));
    let variance:Decimal=sqlx::query_scalar("SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code=(SELECT code FROM accounts WHERE system_role='purchase_return_variance')").fetch_one(&pool).await.unwrap();
    assert_eq!(variance, d("10"));
    let accounts = call(&app, &cookie, "GET", "/api/accounting/accounts", None).await;
    assert!(
        accounts["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["system_role"] == "purchase_return_variance" && a["is_control"] == true)
    );
    let order = call(
        &app,
        &cookie,
        "GET",
        &format!(
            "/api/purchase-orders/{}",
            invoice["order_id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(order["status"], "received");
    assert_eq!(
        amount(&order["items"][0], "remaining_quantity"),
        Decimal::ZERO
    );
    assert_ledger(&pool).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_invalid_roles_quantities_units_and_stock_are_atomic(pool: PgPool) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    let original = snapshot(&pool).await;
    let (_, warehouse) = actor(&pool, "warehouse").await;
    assert_eq!(
        request(
            &app,
            &warehouse,
            "POST",
            &path(&invoice),
            Some(body(&invoice, "1"))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for input in [
        body(&invoice, "3.001"),
        body(&invoice, "0"),
        body(&invoice, "1.0001"),
        body(&invoice, "1\0"),
        {
            let mut value = body(&invoice, "1");
            value["items"][0]["bill_line_id"] = json!(Uuid::new_v4());
            value
        },
        {
            let mut value = body(&invoice, "1");
            value["items"] = json!([value["items"][0], value["items"][0]]);
            value
        },
    ] {
        let (status, _) = request(&app, &cookie, "POST", &path(&invoice), Some(input)).await;
        assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::CONFLICT);
        assert_eq!(snapshot(&pool).await, original);
    }
    sqlx::query("UPDATE products SET unit='box' WHERE id=$1")
        .bind(Uuid::parse_str(id(&product)).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&invoice),
            Some(body(&invoice, "1"))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE products SET unit='kg' WHERE id=$1")
        .bind(Uuid::parse_str(id(&product)).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    call(&app,&cookie,"POST","/api/inventory/adjustments",Some(json!({"product_id":product["id"],"quantity":"-2.5","reason":"Fixture stock loss","idempotency_key":Uuid::new_v4()}))).await;
    let before = snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&invoice),
            Some(body(&invoice, "1"))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, before);
    assert_ledger(&pool).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_concurrent_versions_prevent_double_credit_and_later_movement_blocks_reversal(
    pool: PgPool,
) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    let target = path(&invoice);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &target, Some(body(&invoice, "2"))),
        request(&app, &cookie, "POST", &target, Some(body(&invoice, "2")))
    );
    assert!(a.0.is_success() ^ b.0.is_success());
    assert!(a.0 == StatusCode::CONFLICT || b.0 == StatusCode::CONFLICT);
    let result = if a.0.is_success() { a.1 } else { b.1 };
    assert_eq!(amount(&result, "credit_total"), d("20"));
    call(&app,&cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":product["id"],"quantity":"1","unit_cost":"10","idempotency_key":Uuid::new_v4()}))).await;
    let fresh = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/purchase-bills/{}", id(&invoice)),
        None,
    )
    .await;
    assert_eq!(fresh["returns"][0]["can_reverse"], false);
    let before = snapshot(&pool).await;
    assert_eq!(request(&app,&cookie,"POST",&format!("/api/supplier-returns/{}/reverse",id(&fresh["returns"][0])),Some(json!({"version":fresh["version"],"reason":"Cannot rewind historical cost","idempotency_key":Uuid::new_v4()}))).await.0,StatusCode::CONFLICT);
    assert_eq!(snapshot(&pool).await, before);
    assert_ledger(&pool).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_closed_period_and_refund_reversal_cash_shortage_are_atomic(pool: PgPool) {
    let (app, cookie, product, invoice) = setup(&pool).await;
    fund(&app, &cookie, "30").await;
    let paid = pay(&app, &cookie, &invoice, "30").await;
    let returned = call(&app, &cookie, "POST", &path(&paid), Some(body(&paid, "2"))).await;
    assert_eq!(cash(&pool).await, d("20"));
    call(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Withdraw refund fixture","lines":[{"account_code":"3000","debit":"20","credit":"0"},{"account_code":"1100","debit":"0","credit":"20"}],"idempotency_key":Uuid::new_v4()}))).await;
    let reverse_path = format!(
        "/api/supplier-returns/{}/reverse",
        id(&returned["returns"][0])
    );
    let reverse_input = json!({"version":returned["version"],"reason":"Need enough cash to undo refund","idempotency_key":Uuid::new_v4()});
    let original = snapshot(&pool).await;
    assert_eq!(
        request(&app, &cookie, "POST", &reverse_path, Some(reverse_input))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(snapshot(&pool).await, original);
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE 'Asia/Jakarta')::date WHERE singleton").execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&returned),
            Some(body(&returned, "1"))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(stock(&pool, &product).await.0, d("1"));
    assert_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_return_lines_and_restore_transaction_append_guard_are_immutable(pool: PgPool) {
    let (app, cookie, _, invoice) = setup(&pool).await;
    let returned = call(
        &app,
        &cookie,
        "POST",
        &path(&invoice),
        Some(body(&invoice, "1")),
    )
    .await;
    let source = &returned["returns"][0];
    let source_id = Uuid::parse_str(id(source)).unwrap();
    let update = sqlx::query("UPDATE supplier_return_lines SET credit=credit+1 WHERE return_id=$1")
        .bind(source_id)
        .execute(&pool)
        .await;
    assert!(update.is_err());
    let append = "INSERT INTO supplier_return_lines(id,return_id,bill_line_id,movement_id,position,product_id,product_sku,product_name,unit,quantity,credit,value,unit_cost,before_quantity,before_value,before_average) SELECT $1,$2,bill_line_id,movement_id,position,product_id,product_sku,product_name,unit,quantity,credit,value,unit_cost,before_quantity,before_value,before_average FROM supplier_return_lines WHERE return_id=$3";
    let error = sqlx::query(append)
        .bind(Uuid::new_v4())
        .bind(source_id)
        .bind(source_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        error
            .as_database_error()
            .unwrap()
            .message()
            .contains("append")
    );
    let journal=call(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Restore guard return fixture","lines":[{"account_code":"1100","debit":"1","credit":"0"},{"account_code":"3000","debit":"0","credit":"1"}],"idempotency_key":Uuid::new_v4()}))).await;
    let mut target = pool.begin().await.unwrap();
    let current_xid: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
        .fetch_one(&mut *target)
        .await
        .unwrap();
    let restored = Uuid::new_v4();
    sqlx::query("INSERT INTO supplier_returns(id,number,bill_id,credit_total,inventory_value,refund,reason,journal_id,created_by,posting_transaction) SELECT $1,'PR-RESTORE-GUARD',bill_id,credit_total,inventory_value,refund,reason,$2,created_by,$3::text::xid8 FROM supplier_returns WHERE id=$4")
 .bind(restored).bind(Uuid::parse_str(id(&journal)).unwrap()).bind(current_xid).bind(source_id).execute(&pool).await.unwrap();
    let error = sqlx::query(append)
        .bind(Uuid::new_v4())
        .bind(restored)
        .bind(source_id)
        .execute(&mut *target)
        .await
        .unwrap_err();
    assert!(
        error
            .as_database_error()
            .unwrap()
            .message()
            .contains("append")
    );
    target.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_payment_can_settle_a_full_bill_above_one_billion(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LARGE-PAY-SUP").await;
    let product = product(&app, &cookie, "LARGE-PAY-STOCK").await;
    let draft=call(&app,&cookie,"POST","/api/purchase-orders",Some(json!({"supplier_id":supplier["id"],"items":[{"product_id":product["id"],"quantity":"2","unit_cost":"600000000"}],"idempotency_key":Uuid::new_v4()}))).await;
    let confirmed = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/purchase-orders/{}/confirm", id(&draft)),
        Some(json!({"version":draft["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let received = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/purchase-orders/{}/receive", id(&draft)),
        Some(json!({"version":confirmed["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let invoice = bill(&pool, &app, &cookie, &received, "LARGE-PAY-INVOICE").await;
    fund(&app, &cookie, "1200000000").await;
    let settled = pay(&app, &cookie, &invoice, "1200000000").await;
    assert_eq!(settled["status"], "paid");
    assert_eq!(amount(&settled, "paid"), d("1200000000"));
    assert_eq!(amount(&settled, "outstanding"), Decimal::ZERO);
    assert_eq!(cash(&pool).await, Decimal::ZERO);
    assert_ledger(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn supplier_aging_boundaries_use_company_today_and_partial_outstanding_consistently(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "AGING-BOUNDARY-SUP").await;
    let product = product(&app, &cookie, "AGING-BOUNDARY-STOCK").await;
    let company_today: String =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date::text FROM company")
            .fetch_one(&pool)
            .await
            .unwrap();
    let today = chrono::NaiveDate::parse_from_str(&company_today, "%Y-%m-%d").unwrap();
    let invoice_date = (today - chrono::Days::new(100)).to_string();
    let mut partial = None;
    let mut original_total = Decimal::ZERO;
    for age in [0_u64, 1, 30, 31, 60, 61, 90, 91] {
        let order = crate::supplier_bill_tests::received(&app, &cookie, &supplier, &product).await;
        let mut input =
            crate::supplier_bill_tests::bill_body(&pool, &order, &format!("AGING-BOUNDARY-{age}"))
                .await;
        input["invoice_date"] = json!(invoice_date);
        input["due_date"] = json!((today - chrono::Days::new(age)).to_string());
        let invoice = call(&app, &cookie, "POST", "/api/purchase-bills", Some(input)).await;
        original_total += amount(&invoice, "total");
        if age == 31 {
            partial = Some(invoice);
        }
    }
    fund(&app, &cookie, "10").await;
    let partial = pay(&app, &cookie, &partial.unwrap(), "10").await;
    let analysis = call(
        &app,
        &cookie,
        "GET",
        "/api/purchase-bills/analysis?months=6",
        None,
    )
    .await;
    assert_eq!(analysis["today"], company_today);
    assert_eq!(analysis["timezone"], "Asia/Jakarta");
    let bucket = |key: &str| {
        analysis["aging"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["key"] == key)
            .unwrap()
    };
    for (key, count, total) in [
        ("current", 1, "13.89"),
        ("days_1_30", 2, "27.78"),
        ("days_31_60", 2, "17.78"),
        ("days_61_90", 2, "27.78"),
        ("over_90", 1, "13.89"),
    ] {
        assert_eq!(bucket(key)["count"], count);
        assert_eq!(amount(bucket(key), "total"), d(total));
    }
    let entry = analysis["bills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == partial["id"])
        .unwrap();
    assert_eq!(entry["bucket"], "days_31_60");
    assert_eq!(entry["days_overdue"], 31);
    assert_eq!(amount(entry, "outstanding"), d("3.89"));
    let aging_total: Decimal = analysis["aging"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| amount(row, "total"))
        .sum();
    assert_eq!(aging_total, original_total - d("10"));
    let summary = call(&app, &cookie, "GET", "/api/purchase-bills/summary", None).await;
    assert_eq!(amount(&summary, "outstanding_total"), aging_total);
    assert_eq!(summary["counts"]["unpaid"], 8);
    assert_eq!(summary["counts"]["partial"], 1);
    assert_eq!(amount(&summary, "overdue_total"), aging_total - d("13.89"));
    let list = call(&app, &cookie, "GET", "/api/purchase-bills", None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 8);
    assert_eq!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| amount(row, "outstanding"))
            .sum::<Decimal>(),
        aging_total
    );
    assert_ledger(&pool).await;
}
