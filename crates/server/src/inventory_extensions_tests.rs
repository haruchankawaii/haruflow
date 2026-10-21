use crate::supplier_bill_tests::{actor, call, fixture as base_fixture, request};
use axum::Router;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::borrow::Cow;
use uuid::Uuid;
async fn fixture(pool: &PgPool) -> (Router, Uuid, String) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("haruflow=trace")
        .with_test_writer()
        .try_init();
    base_fixture(pool).await
}
fn uid(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}
fn val(v: &Value, k: &str) -> Decimal {
    v[k].as_str().unwrap().parse().unwrap()
}
async fn product(app: &Router, cookie: &str, sku: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/products",
        Some(json!({"sku":sku,"name":sku,"unit":"pcs","price":"30"})),
    )
    .await
}
async fn warehouse(app: &Router, cookie: &str) -> Value {
    call(app,cookie,"POST","/api/inventory/warehouses",Some(json!({"code":Uuid::new_v4().simple().to_string(),"name":"Gudang cabang","idempotency_key":Uuid::new_v4()}))).await
}
async fn main_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM warehouses WHERE is_default")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn positions(app: &Router, cookie: &str, p: Uuid) -> Value {
    call(
        app,
        cookie,
        "GET",
        &format!("/api/inventory/positions?product_id={p}"),
        None,
    )
    .await
}
async fn receive(
    app: &Router,
    cookie: &str,
    p: Uuid,
    w: Uuid,
    quantity: &str,
    cost: &str,
    lot: Option<Uuid>,
) -> Value {
    call(app,cookie,"POST","/api/inventory/location-receipts",Some(json!({"product_id":p,"warehouse_id":w,"lot_id":lot,"quantity":quantity,"unit_cost":cost,"idempotency_key":Uuid::new_v4()}))).await
}
async fn totals(pool: &PgPool, p: Uuid) -> (Decimal, Decimal, Decimal, Decimal) {
    sqlx::query_as("SELECT p.quantity,p.inventory_value,COALESCE(sum(b.quantity),0),COALESCE(sum(b.inventory_value),0) FROM products p LEFT JOIN stock_buckets b ON b.product_id=p.id WHERE p.id=$1 GROUP BY p.id").bind(p).fetch_one(pool).await.unwrap()
}
async fn create_order(app: &Router, cookie: &str, p: Uuid, quantity: &str) -> Value {
    let c = call(
        app,
        cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"Pelanggan reservasi"})),
    )
    .await;
    let o=call(app,cookie,"POST","/api/orders",Some(json!({"customer_id":c["id"],"items":[{"product_id":p,"quantity":quantity,"unit_price":"30"}],"idempotency_key":Uuid::new_v4()}))).await;
    call(
        app,
        cookie,
        "POST",
        &format!("/api/orders/{}/confirm", uid(&o)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await
}
async fn lot(
    app: &Router,
    cookie: &str,
    p: Uuid,
    code: &str,
    kind: &str,
    expires: Option<&str>,
) -> Value {
    call(app,cookie,"POST","/api/inventory/lots",Some(json!({"product_id":p,"code":code,"kind":kind,"expires_on":expires,"idempotency_key":Uuid::new_v4()}))).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn warehouses_transfers_keep_pooled_moving_average_and_global_ledger_exact(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let p = uid(&product(&app, &cookie, "WAREHOUSE-COST").await);
    let main = main_id(&pool).await;
    let branch = uid(&warehouse(&app, &cookie).await);
    receive(&app, &cookie, p, main, "100", "10", None).await;
    let before_journals: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    let pos = positions(&app, &cookie, p).await;
    let source = &pos["items"][0];
    let body = json!({"from_warehouse_id":main,"to_warehouse_id":branch,"items":[{"product_id":p,"quantity":"50","version":source["version"]}],"reason":"Distribusi persediaan cabang","idempotency_key":Uuid::new_v4()});
    let transferred = call(
        &app,
        &cookie,
        "POST",
        "/api/inventory/transfers",
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        transferred,
        call(
            &app,
            &cookie,
            "POST",
            "/api/inventory/transfers",
            Some(body)
        )
        .await
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap(),
        before_journals,
        "Transfer must not create revenue, expense or artificial inventory journals"
    );
    receive(&app, &cookie, p, branch, "50", "20", None).await;
    let (q, v, lq, lv) = totals(&pool, p).await;
    assert_eq!((q, v), (Decimal::from(150), Decimal::from(2000)));
    assert_eq!((q, v), (lq, lv));
    let pos = positions(&app, &cookie, p).await;
    let primary = pos["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["warehouse_id"] == main.to_string())
        .unwrap();
    assert_eq!(val(primary, "quantity"), Decimal::from(50));
    assert_eq!(val(primary, "inventory_value"), "666.67".parse().unwrap());
    assert!(
        sqlx::query("UPDATE products SET quantity=quantity+1 WHERE id=$1")
            .bind(p)
            .execute(&pool)
            .await
            .is_err(),
        "Location/global balance is a deferred database invariant"
    );
    assert_eq!(totals(&pool, p).await, (q, v, lq, lv));
}

#[sqlx::test(migrations = "../../migrations")]
async fn reservations_protect_legacy_shipments_and_are_consumed_only_by_their_order(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let p = uid(&product(&app, &cookie, "RESERVATION").await);
    let main = main_id(&pool).await;
    receive(&app, &cookie, p, main, "10", "10", None).await;
    let a = create_order(&app, &cookie, p, "6").await;
    let b = create_order(&app, &cookie, p, "5").await;
    let pos = positions(&app, &cookie, p).await;
    let held=call(&app,&cookie,"POST","/api/inventory/reservations",Some(json!({"product_id":p,"warehouse_id":main,"quantity":"6","version":pos["items"][0]["version"],"order_id":a["id"],"idempotency_key":Uuid::new_v4()}))).await;
    let (status, _) = request(
        &app,
        &cookie,
        "POST",
        &format!("/api/orders/{}/ship", uid(&b)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert!(status.is_client_error());
    assert_eq!(totals(&pool, p).await.0, Decimal::from(10));
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/orders/{}/ship", uid(&a)),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(totals(&pool, p).await.0, Decimal::from(4));
    let remaining: Decimal = sqlx::query_scalar("SELECT stock_reservation_remaining($1)")
        .bind(uid(&held))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, Decimal::ZERO);
    let (_, viewer) = actor(&pool, "viewer").await;
    assert_eq!(
        request(
            &app,
            &viewer,
            "POST",
            "/api/inventory/warehouses",
            Some(json!({"code":"NO","name":"Forbidden","idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0,
        403
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn serials_expiry_and_exact_unit_conversions_reject_partial_or_fabricated_stock(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let main = main_id(&pool).await;
    let p = uid(&product(&app, &cookie, "BATCH").await);
    call(&app,&cookie,"PUT",&format!("/api/inventory/products/{p}/settings"),Some(json!({"version":1,"tracking":"lot","expiry_required":true,"idempotency_key":Uuid::new_v4()}))).await;
    let old = uid(&lot(&app, &cookie, p, "EXPIRED", "lot", Some("2000-01-01")).await);
    let fresh = uid(&lot(&app, &cookie, p, "FRESH", "lot", Some("2099-01-01")).await);
    receive(&app, &cookie, p, main, "1", "10", Some(old)).await;
    receive(&app, &cookie, p, main, "1", "10", Some(fresh)).await;
    let order = create_order(&app, &cookie, p, "2").await;
    assert!(
        request(
            &app,
            &cookie,
            "POST",
            &format!("/api/orders/{}/ship", uid(&order)),
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(totals(&pool, p).await.0, Decimal::from(2));
    let serial = uid(&product(&app, &cookie, "SERIAL").await);
    call(&app,&cookie,"PUT",&format!("/api/inventory/products/{serial}/settings"),Some(json!({"version":1,"tracking":"serial","expiry_required":false,"idempotency_key":Uuid::new_v4()}))).await;
    let s = uid(&lot(&app, &cookie, serial, "SN-001", "serial", None).await);
    assert!(request(&app,&cookie,"POST","/api/inventory/location-receipts",Some(json!({"product_id":serial,"warehouse_id":main,"lot_id":s,"quantity":"2","unit_cost":"10","idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    receive(&app, &cookie, serial, main, "1", "10", Some(s)).await;
    assert!(request(&app,&cookie,"POST","/api/inventory/location-receipts",Some(json!({"product_id":serial,"warehouse_id":main,"lot_id":s,"quantity":"1","unit_cost":"10","idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    let converted = uid(&product(&app, &cookie, "CONVERSION").await);
    call(&app,&cookie,"POST",&format!("/api/inventory/products/{converted}/units"),Some(json!({"name":"box","numerator":"12","denominator":"1","idempotency_key":Uuid::new_v4()}))).await;
    call(&app,&cookie,"POST","/api/inventory/location-receipts",Some(json!({"product_id":converted,"warehouse_id":main,"quantity":"2","unit":"box","unit_cost":"120","idempotency_key":Uuid::new_v4()}))).await;
    assert_eq!(totals(&pool, converted).await.0, Decimal::from(24));
    assert_eq!(totals(&pool, converted).await.1, Decimal::from(240));
    call(&app,&cookie,"POST",&format!("/api/inventory/products/{converted}/units"),Some(json!({"name":"third","numerator":"1","denominator":"3","idempotency_key":Uuid::new_v4()}))).await;
    assert!(request(&app,&cookie,"POST","/api/inventory/location-receipts",Some(json!({"product_id":converted,"warehouse_id":main,"quantity":"1","unit":"third","unit_cost":"10","idempotency_key":Uuid::new_v4()}))).await.0.is_client_error());
    assert_eq!(totals(&pool, converted).await.0, Decimal::from(24));
}

#[sqlx::test(migrations = false)]
async fn warehouse_upgrade_preserves_old_journals_and_movements_without_guessing_historical_lots(
    pool: PgPool,
) {
    let mut prior = sqlx::migrate!("../../migrations");
    prior.migrations = Cow::Owned(prior.iter().filter(|m| m.version <= 25).cloned().collect());
    prior.run(&pool).await.unwrap();
    let (app, _, cookie) = fixture(&pool).await;
    let p = uid(&product(&app, &cookie, "LEGACY-LOCATION").await);
    call(&app,&cookie,"POST","/api/inventory/receipts",Some(json!({"product_id":p,"quantity":"7","unit_cost":"3.33","idempotency_key":Uuid::new_v4()}))).await;
    let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('journal',(SELECT jsonb_agg(to_jsonb(j)-'posted_at'-'reporting_kind' ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l),'movements',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM inventory_movements m),'products',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM products p))").fetch_one(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('journal',(SELECT jsonb_agg(to_jsonb(j)-'posted_at'-'reporting_kind' ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l),'movements',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM inventory_movements m),'products',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM products p))").fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    let (q, v, lq, lv) = totals(&pool, p).await;
    assert_eq!((q, v), (lq, lv));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM stock_lots")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let rows = positions(&app, &cookie, p).await;
    assert_eq!(rows["items"][0]["lot_id"], Value::Null);
}
