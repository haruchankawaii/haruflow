use crate::supplier_bill_tests::{call, fixture, product};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn report(pool: &PgPool) -> Value {
    serde_json::to_value(crate::database_health::check(pool).await.unwrap()).unwrap()
}
fn failed(value: &Value, name: &str) -> bool {
    value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["name"] == name && check["passed"] == false)
}

#[sqlx::test(migrations = "../../migrations")]
async fn fresh_database_and_posted_stock_are_healthy_without_diagnostic_writes(pool: PgPool) {
    let empty = report(&pool).await;
    assert_eq!(empty["healthy"], true, "{empty}");
    assert_eq!(empty["read_only"], true);
    let (app, _, cookie) = fixture(&pool).await;
    let p = product(&app, &cookie, "HEALTH-STOCK").await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/inventory/receipts",
        Some(json!({
            "product_id":p["id"],"quantity":"10","unit_cost":"12.50",
            "reference":"Diagnostic simulation","idempotency_key":Uuid::new_v4()
        })),
    )
    .await;
    let before: Value = sqlx::query_scalar("SELECT jsonb_build_object('journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'stock',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM products p),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM audit_events a))").fetch_one(&pool).await.unwrap();
    let result = report(&pool).await;
    assert_eq!(result["healthy"], true, "{result}");
    let after: Value = sqlx::query_scalar("SELECT jsonb_build_object('journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'stock',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM products p),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM audit_events a))").fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "Diagnostics must not modify business or audit data"
    );
    assert!(!result.to_string().contains("postgres://"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabled_protection_and_stock_corruption_are_reported_without_repairs(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let p = product(&app, &cookie, "HEALTH-CORRUPTION").await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/inventory/receipts",
        Some(json!({
            "product_id":p["id"],"quantity":"10","unit_cost":"12.50",
            "idempotency_key":Uuid::new_v4()
        })),
    )
    .await;
    // Deliberately damage only this disposable test database as its operator.
    sqlx::query("ALTER TABLE journal_lines DISABLE TRIGGER immutable_journal_lines")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE products DISABLE TRIGGER product_stock_location_balance")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE products SET inventory_value=inventory_value+1 WHERE id=$1")
        .bind(p["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let result = report(&pool).await;
    assert_eq!(result["healthy"], false);
    assert!(failed(&result, "enabled_triggers"));
    assert!(failed(&result, "required_ledger_guards"));
    assert!(failed(&result, "stock_location_totals"));
    assert!(failed(&result, "inventory_control"));
    let state: String = sqlx::query_scalar(
        "SELECT tgenabled::text FROM pg_trigger WHERE tgname='immutable_journal_lines'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        state, "D",
        "Diagnostics must not silently re-enable protections"
    );
    let value: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT inventory_value FROM products WHERE id=$1")
            .bind(p["id"].as_str().unwrap().parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(value, rust_decimal::Decimal::from(126));
}

#[sqlx::test(migrations = "../../migrations")]
async fn checksum_mismatch_stops_schema_dependent_checks_without_repair(pool: PgPool) {
    sqlx::query("UPDATE _sqlx_migrations SET checksum=decode('00','hex') WHERE version=1")
        .execute(&pool)
        .await
        .unwrap();
    let result = report(&pool).await;
    assert_eq!(result["healthy"], false);
    assert_eq!(result["checks"].as_array().unwrap().len(), 1);
    assert!(failed(&result, "migration_checksums"));
    let checksum: String =
        sqlx::query_scalar("SELECT encode(checksum,'hex') FROM _sqlx_migrations WHERE version=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checksum, "00");
}
