use crate::supplier_bill_tests::{actor, bill_body, call, fixture, product, request, supplier};
use axum::Router;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn uid(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}
fn d(v: &str) -> Decimal {
    v.parse().unwrap()
}
fn money(v: &Value, key: &str) -> Decimal {
    v[key].as_str().unwrap().parse().unwrap()
}
fn receipt(order: &Value) -> &Value {
    order["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == order["active_receipt_id"])
        .unwrap()
}
async fn received(app: &Router, cookie: &str, party: &Value, items: Value) -> Value {
    let draft = call(
        app,
        cookie,
        "POST",
        "/api/purchase-orders",
        Some(json!({"supplier_id":party["id"],"items":items,"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let confirmed = call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-orders/{}/confirm", uid(&draft)),
        Some(json!({"version":draft["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/purchase-orders/{}/receive", uid(&confirmed)),
        Some(json!({"version":confirmed["version"],"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn billed(pool: &PgPool, app: &Router, cookie: &str, order: &Value, shipping: &str) -> Value {
    let mut body = bill_body(pool, order, &format!("LC-{}", Uuid::new_v4())).await;
    body["shipping"] = json!(shipping);
    call(app, cookie, "POST", "/api/purchase-bills", Some(body)).await
}
async fn ship(app: &Router, cookie: &str, p: &Value, quantity: &str) -> Value {
    let customer = call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Landed cost customer"})),
    )
    .await;
    let order=call(app,cookie,"POST","/api/orders",Some(json!({"customer_id":customer["id"],"items":[{"product_id":p["id"],"quantity":quantity,"unit_price":"30"}],"idempotency_key":Uuid::new_v4()}))).await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/orders/{}/confirm", uid(&order)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/orders/{}/ship", uid(&order)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn find_source(app: &Router, cookie: &str, document: &Value, role: &str) -> Value {
    call(app, cookie, "GET", "/api/landed-costs/options", None).await["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source_id"] == document["id"] && s["system_role"] == role)
        .unwrap()
        .clone()
}
fn body(source: &Value, lines: Vec<Value>, amount: &str, method: &str) -> Value {
    json!({"source_line_id":source["line_id"],"amount":amount,"allocation_method":method,"receipt_line_ids":lines,"reference":"Freight invoice and cost review","reason":"Alokasi biaya pembelian yang sudah dicatat","idempotency_key":Uuid::new_v4()})
}
async fn reviewed(app: &Router, cookie: &str, mut body: Value) -> Value {
    let preview = call(
        app,
        cookie,
        "POST",
        "/api/landed-costs/preview",
        Some(body.clone()),
    )
    .await;
    body["expected_preview"] = preview["preview_token"].clone();
    body
}
async fn reverse(app: &Router, cookie: &str, cost: &Value) -> Value {
    call(app,cookie,"POST",&format!("/api/landed-costs/{}/reverse",uid(cost)),Some(json!({"version":cost["version"],"reason":"Biaya akan dialokasikan ulang sesuai bukti","idempotency_key":Uuid::new_v4()}))).await
}
async fn totals(pool: &PgPool, p: &Value) -> (Decimal, Decimal, Decimal, Decimal) {
    sqlx::query_as("SELECT p.quantity,p.inventory_value,COALESCE(sum(b.quantity),0),COALESCE(sum(b.inventory_value),0) FROM products p LEFT JOIN stock_buckets b ON b.product_id=p.id WHERE p.id=$1 GROUP BY p.id")
        .bind(uid(p)).fetch_one(pool).await.unwrap()
}
async fn ledger(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('headers',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l),'receipts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM purchase_receipts r),'receipt_lines',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM purchase_receipt_lines r),'bills',(SELECT jsonb_agg(to_jsonb(b) ORDER BY id) FROM purchase_bills b),'movements',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM inventory_movements m))").fetch_one(pool).await.unwrap()
}
async fn assert_balanced(pool: &PgPool) {
    let unbalanced:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit))j").fetch_one(pool).await.unwrap();
    assert_eq!(unbalanced, 0);
    let (stock,ledger):(Decimal,Decimal)=sqlx::query_as("SELECT (SELECT COALESCE(sum(inventory_value),0) FROM products),(SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1300')").fetch_one(pool).await.unwrap();
    assert_eq!(stock, ledger);
}

#[sqlx::test(migrations = "../../migrations")]
async fn late_cost_splits_sold_stock_reconciles_locations_and_reverses_without_rewriting_history(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LC-PARTIAL").await;
    let p = product(&app, &cookie, "LC-PARTIAL").await;
    let order = received(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":p["id"],"quantity":"10","unit_cost":"10"}]),
    )
    .await;
    let bill = billed(&pool, &app, &cookie, &order, "100").await;
    let sale = ship(&app, &cookie, &p, "4").await;
    let before = ledger(&pool).await;
    let source = find_source(&app, &cookie, &bill, "purchase_shipping_expense").await;
    let unreviewed = body(
        &source,
        vec![receipt(&order)["items"][0]["id"].clone()],
        "100",
        "value",
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/landed-costs",
            Some(unreviewed.clone())
        )
        .await
        .0,
        409
    );
    let body = reviewed(&app, &cookie, unreviewed).await;
    let posted = call(
        &app,
        &cookie,
        "POST",
        "/api/landed-costs",
        Some(body.clone()),
    )
    .await;
    assert_eq!(money(&posted, "inventory_amount"), d("60"));
    assert_eq!(money(&posted, "expense_amount"), d("40"));
    let history = call(
        &app,
        &cookie,
        "GET",
        &format!(
            "/api/records/landed-costs?q={}&status=posted",
            posted["number"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["id"], posted["id"]);
    assert_eq!(
        history["items"][0]["url"],
        format!("?landed_cost={}#purchasing", uid(&posted))
    );
    assert_eq!(money(&posted["items"][0], "remaining_ratio"), d("0.6"));
    assert_eq!(
        totals(&pool, &p).await,
        (d("6"), d("120"), d("6"), d("120"))
    );
    let cost: (Decimal, Decimal) =
        sqlx::query_as("SELECT average_cost,inventory_value FROM products WHERE id=$1")
            .bind(uid(&p))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cost, (d("20"), d("120")));
    assert!(!posted["location_events"].as_array().unwrap().is_empty());
    assert_eq!(
        posted,
        call(
            &app,
            &cookie,
            "POST",
            "/api/landed-costs",
            Some(body.clone())
        )
        .await
    );
    let mut different = body.clone();
    different["reason"] = json!("Alasan berbeda pada kunci lama");
    assert_eq!(
        request(&app, &cookie, "POST", "/api/landed-costs", Some(different))
            .await
            .0,
        409
    );
    let after = ledger(&pool).await;
    for key in ["headers", "lines"] {
        for original in before[key].as_array().unwrap() {
            assert!(after[key].as_array().unwrap().contains(original));
        }
    }
    for key in ["receipts", "receipt_lines", "bills", "movements"] {
        assert_eq!(
            before[key], after[key],
            "Original {key} must stay unchanged"
        );
    }
    assert_eq!(
        call(
            &app,
            &cookie,
            "GET",
            &format!("/api/purchase-bills/{}", uid(&bill)),
            None
        )
        .await["can_reverse"],
        false
    );
    let (status, _) = request(
        &app,
        &cookie,
        "POST",
        &format!("/api/orders/{}/reverse-shipment", uid(&sale)),
        Some(json!({"reason":"Pembalikan pengiriman historis","idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert!(
        status.is_client_error(),
        "Historical shipment cost attribution must remain protected"
    );
    let invoice = call(
        &app,
        &cookie,
        "POST",
        &format!("/api/orders/{}/invoice", uid(&sale)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    let return_body = json!({"version":invoice["version"],"reason":"Retur barang pengiriman sebelum biaya tambahan","items":[{"invoice_line_id":invoice["items"][0]["id"],"quantity":"1"}],"idempotency_key":Uuid::new_v4()});
    let before_return = ledger(&pool).await;
    let (blocked_status, blocked_return) = request(
        &app,
        &cookie,
        "POST",
        &format!("/api/invoices/{}/returns", uid(&invoice)),
        Some(return_body.clone()),
    )
    .await;
    assert_eq!(blocked_status, 409);
    assert!(
        blocked_return["error"]["message"]
            .as_str()
            .unwrap()
            .contains("biaya perolehan")
    );
    assert_eq!(
        ledger(&pool).await,
        before_return,
        "Rejected historical-cost return rolls back every financial and stock posting"
    );
    assert_balanced(&pool).await;
    let reversed = reverse(&app, &cookie, &posted).await;
    assert_eq!(reversed["status"], "reversed");
    assert_eq!(reversed["version"], 2);
    let reversed_history = call(
        &app,
        &cookie,
        "GET",
        &format!(
            "/api/records/landed-costs?q={}&status=reversed",
            posted["number"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(reversed_history["items"][0]["id"], posted["id"]);
    assert_eq!(totals(&pool, &p).await, (d("6"), d("60"), d("6"), d("60")));
    assert_eq!(
        money(
            &find_source(&app, &cookie, &bill, "purchase_shipping_expense").await,
            "available"
        ),
        d("100")
    );
    assert_balanced(&pool).await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/invoices/{}/returns", uid(&invoice)),
        Some(return_body),
    )
    .await;
    assert_eq!(totals(&pool, &p).await, (d("7"), d("70"), d("7"), d("70")));
    assert_balanced(&pool).await;
    assert!(
        sqlx::query("UPDATE landed_cost_lines SET allocated=allocated+1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("TRUNCATE landed_cost_location_events")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO landed_cost_lines SELECT gen_random_uuid(),landed_cost_id,999,receipt_line_id,product_id,product_sku,product_name,unit,receipt_number,quantity,receipt_value,weight,remaining_ratio,allocated,inventory_amount,expense_amount,before_quantity,before_value,before_average,after_value,after_average,receipt_movement_seq,cutoff_movement_seq,cutoff_location_seq FROM landed_cost_lines LIMIT 1").execute(&pool).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn cent_allocation_cumulative_rounding_and_multiple_documents_unwind_in_dependency_order(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LC-CENTS").await;
    let a = product(&app, &cookie, "LC-CENTS-A").await;
    let b = product(&app, &cookie, "LC-CENTS-B").await;
    let order=received(&app,&cookie,&supplier,json!([{"product_id":a["id"],"quantity":"1","unit_cost":"1"},{"product_id":b["id"],"quantity":"1","unit_cost":"2"}])).await;
    let bill = billed(&pool, &app, &cookie, &order, "0.11").await;
    let source = find_source(&app, &cookie, &bill, "purchase_shipping_expense").await;
    let lines = receipt(&order)["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].clone())
        .collect::<Vec<_>>();
    let first = call(
        &app,
        &cookie,
        "POST",
        "/api/landed-costs",
        Some(reviewed(&app, &cookie, body(&source, lines.clone(), "0.05", "value")).await),
    )
    .await;
    let allocated: Decimal = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| money(l, "allocated"))
        .sum();
    assert_eq!(allocated, d("0.05"));
    for line in first["items"].as_array().unwrap() {
        assert_eq!(
            money(line, "allocated"),
            if money(line, "receipt_value") == d("1") {
                d("0.02")
            } else {
                d("0.03")
            }
        );
    }
    let second = call(
        &app,
        &cookie,
        "POST",
        "/api/landed-costs",
        Some(reviewed(&app, &cookie, body(&source, lines, "0.06", "quantity")).await),
    )
    .await;
    for line in second["items"].as_array().unwrap() {
        assert_eq!(money(line, "allocated"), d("0.03"));
    }
    assert_eq!(
        call(
            &app,
            &cookie,
            "GET",
            &format!("/api/landed-costs/{}", uid(&first)),
            None
        )
        .await["can_reverse"],
        false
    );
    reverse(&app, &cookie, &second).await;
    assert_eq!(
        call(
            &app,
            &cookie,
            "GET",
            &format!("/api/landed-costs/{}", uid(&first)),
            None
        )
        .await["can_reverse"],
        true
    );
    reverse(&app, &cookie, &first).await;
    assert_eq!(totals(&pool, &a).await.1, d("1"));
    assert_eq!(totals(&pool, &b).await.1, d("2"));
    assert_balanced(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_source_capacity_preview_changes_roles_period_and_later_stock_are_guarded(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LC-RACE").await;
    let p = product(&app, &cookie, "LC-RACE").await;
    let order = received(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":p["id"],"quantity":"10","unit_cost":"10"}]),
    )
    .await;
    let bill = billed(&pool, &app, &cookie, &order, "100").await;
    let source = find_source(&app, &cookie, &bill, "purchase_shipping_expense").await;
    let lines = vec![receipt(&order)["items"][0]["id"].clone()];
    let first = reviewed(&app, &cookie, body(&source, lines.clone(), "80", "value")).await;
    let mut second = first.clone();
    second["idempotency_key"] = json!(Uuid::new_v4());
    let ((sa, _), (sb, _)) = tokio::join!(
        request(&app, &cookie, "POST", "/api/landed-costs", Some(first)),
        request(&app, &cookie, "POST", "/api/landed-costs", Some(second))
    );
    assert!(sa.is_success() != sb.is_success());
    assert!(sa == 409 || sb == 409);
    let stale = reviewed(&app, &cookie, body(&source, lines.clone(), "20", "value")).await;
    ship(&app, &cookie, &p, "1").await;
    assert_eq!(
        request(&app, &cookie, "POST", "/api/landed-costs", Some(stale))
            .await
            .0,
        409
    );
    let costs = call(&app, &cookie, "GET", "/api/landed-costs", None).await;
    let original = &costs["items"][0];
    assert_eq!(original["can_reverse"], false);
    let (_, viewer) = actor(&pool, "viewer").await;
    let (_, sales) = actor(&pool, "sales").await;
    assert!(
        request(&app, &viewer, "GET", "/api/landed-costs", None)
            .await
            .0
            .is_success()
    );
    for role in [&viewer, &sales] {
        assert_eq!(
            request(
                &app,
                role,
                "POST",
                "/api/landed-costs/preview",
                Some(body(&source, lines.clone(), "20", "value"))
            )
            .await
            .0,
            403
        );
    }
    let pending = reviewed(&app, &cookie, body(&source, lines, "20", "value")).await;
    sqlx::query("UPDATE accounting_period SET closed_through=(now() AT TIME ZONE 'Asia/Jakarta')::date WHERE singleton").execute(&pool).await.unwrap();
    assert_eq!(
        request(&app, &cookie, "POST", "/api/landed-costs", Some(pending))
            .await
            .0,
        409
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM landed_costs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_balanced(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_service_cost_is_eligible_but_service_targets_and_ambiguous_returns_are_rejected(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LC-SERVICE").await;
    let p = product(&app, &cookie, "LC-GOODS").await;
    let service=call(&app,&cookie,"POST","/api/products",Some(json!({"sku":"LC-SERVICE","name":"Freight forwarder","kind":"service","unit":"job","price":"30"}))).await;
    let goods = received(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":p["id"],"quantity":"10","unit_cost":"10"}]),
    )
    .await;
    let cost = received(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":service["id"],"quantity":"1","unit_cost":"30"}]),
    )
    .await;
    let provisional = find_source(&app, &cookie, receipt(&cost), "purchase_service_expense").await;
    assert_eq!(
        provisional["eligible"], false,
        "Service expense must be final before capitalization"
    );
    let service_bill = billed(&pool, &app, &cookie, &cost, "0").await;
    let source = find_source(&app, &cookie, receipt(&cost), "purchase_service_expense").await;
    assert!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/landed-costs/preview",
            Some(body(
                &source,
                vec![receipt(&cost)["items"][0]["id"].clone()],
                "30",
                "value"
            ))
        )
        .await
        .0
        .is_client_error()
    );
    let posted = call(
        &app,
        &cookie,
        "POST",
        "/api/landed-costs",
        Some(
            reviewed(
                &app,
                &cookie,
                body(
                    &source,
                    vec![receipt(&goods)["items"][0]["id"].clone()],
                    "30",
                    "value",
                ),
            )
            .await,
        ),
    )
    .await;
    assert_eq!(money(&posted, "inventory_amount"), d("30"));
    let blocked: Option<String> = sqlx::query_scalar("SELECT purchase_receipt_blocked_reason($1)")
        .bind(uid(receipt(&cost)))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(blocked.unwrap().contains("landed"));
    let return_body = json!({"version":service_bill["version"],"reason":"Retur biaya jasa sumber","items":[{"bill_line_id":service_bill["items"][0]["id"],"quantity":"1"}],"idempotency_key":Uuid::new_v4()});
    assert!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("/api/purchase-bills/{}/returns", uid(&service_bill)),
            Some(return_body.clone())
        )
        .await
        .0
        .is_client_error()
    );
    reverse(&app, &cookie, &posted).await;
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/purchase-bills/{}/returns", uid(&service_bill)),
        Some(return_body),
    )
    .await;
    let options = call(&app, &cookie, "GET", "/api/landed-costs/options", None).await;
    assert_eq!(
        options["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["line_id"] == source["line_id"])
            .unwrap()["eligible"],
        false
    );
    assert_balanced(&pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn depleted_stock_expenses_all_cost_and_replenishment_does_not_recapitalize_old_sales(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let supplier = supplier(&app, &cookie, "LC-EMPTY").await;
    let p = product(&app, &cookie, "LC-EMPTY").await;
    let order = received(
        &app,
        &cookie,
        &supplier,
        json!([{"product_id":p["id"],"quantity":"3","unit_cost":"10"}]),
    )
    .await;
    let bill = billed(&pool, &app, &cookie, &order, "10").await;
    let source = find_source(&app, &cookie, &bill, "purchase_shipping_expense").await;
    ship(&app, &cookie, &p, "3").await;
    let posted = call(
        &app,
        &cookie,
        "POST",
        "/api/landed-costs",
        Some(
            reviewed(
                &app,
                &cookie,
                body(
                    &source,
                    vec![receipt(&order)["items"][0]["id"].clone()],
                    "10",
                    "value",
                ),
            )
            .await,
        ),
    )
    .await;
    assert_eq!(money(&posted, "inventory_amount"), Decimal::ZERO);
    assert_eq!(money(&posted, "expense_amount"), d("10"));
    assert_eq!(totals(&pool, &p).await.1, Decimal::ZERO);
    call(&app,&cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":p["id"],"quantity":"2","unit_cost":"20","idempotency_key":Uuid::new_v4()}))).await;
    reverse(&app, &cookie, &posted).await;
    assert_eq!(totals(&pool, &p).await.1, d("40"));
    assert_balanced(&pool).await;
}
