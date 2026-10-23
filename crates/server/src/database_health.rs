//! Operator diagnostics: one bounded, read-only snapshot; no automatic repairs.
use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Serialize)]
pub(crate) struct Check {
    name: &'static str,
    passed: bool,
    issues: i64,
}

#[derive(Serialize)]
pub(crate) struct Report {
    pub healthy: bool,
    read_only: bool,
    checked_at: DateTime<Utc>,
    schema_version: i64,
    checks: Vec<Check>,
    warnings: Vec<&'static str>,
}

impl Report {
    fn record(&mut self, name: &'static str, issues: i64) {
        self.healthy &= issues == 0;
        self.checks.push(Check {
            name,
            passed: issues == 0,
            issues,
        });
    }
}

async fn count(tx: &mut Transaction<'_, Postgres>, query: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(query).fetch_one(&mut **tx).await?)
}

pub(crate) async fn check(pool: &PgPool) -> Result<Report> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut *tx)
        .await?;
    let mut report = Report {
        healthy: true,
        read_only: true,
        checked_at: sqlx::query_scalar("SELECT now()")
            .fetch_one(&mut *tx)
            .await?,
        schema_version: crate::runtime::MIGRATIONS
            .iter()
            .last()
            .map(|m| m.version)
            .unwrap_or(0),
        checks: vec![],
        warnings: vec![],
    };
    let compatible = crate::runtime::compatible_connection(&mut tx).await;
    report.record("migration_checksums", i64::from(!compatible));
    if !compatible {
        // Do not run schema-dependent SQL after a missing/incompatible migration.
        // A failed catalog query can also abort PostgreSQL's transaction; rollback
        // remains safe and does not suppress the actionable migration result.
        tx.rollback().await?;
        return Ok(report);
    }
    for (name, query) in [
        (
            "validated_constraints",
            "SELECT count(*) FROM pg_constraint WHERE connamespace='public'::regnamespace AND contype IN ('c','f') AND NOT convalidated",
        ),
        (
            "valid_indexes",
            "SELECT count(*) FROM pg_index i JOIN pg_class t ON t.oid=i.indrelid WHERE t.relnamespace='public'::regnamespace AND (NOT i.indisvalid OR NOT i.indisready)",
        ),
        (
            "enabled_triggers",
            "SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace='public'::regnamespace AND t.tgenabled NOT IN ('O','A')",
        ),
        (
            "required_ledger_guards",
            "SELECT count(*) FROM (VALUES ('journal_entries','immutable_journal'),('journal_lines','immutable_journal_lines'),('journal_lines','journal_original_transaction'),('journal_entries','journal_header_balance'),('journal_lines','journal_line_balance'),('inventory_movements','immutable_inventory'),('audit_events','audit_events_append_only'),('users','community_user_capacity'),('products','product_stock_location_balance'),('stock_buckets','bucket_stock_location_balance')) expected(table_name,trigger_name) WHERE NOT EXISTS(SELECT 1 FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace='public'::regnamespace AND c.relname=expected.table_name AND t.tgname=expected.trigger_name AND t.tgenabled IN ('O','A'))",
        ),
        (
            "balanced_journals",
            "SELECT count(*) FROM (SELECT j.id FROM journal_entries j LEFT JOIN journal_lines l ON l.journal_id=j.id GROUP BY j.id HAVING count(l.id)<2 OR COALESCE(sum(l.debit-l.credit),0)<>0) invalid",
        ),
        (
            "stock_location_totals",
            "SELECT count(*) FROM products p LEFT JOIN (SELECT product_id,sum(quantity) quantity,sum(inventory_value) value FROM stock_buckets GROUP BY product_id)b ON b.product_id=p.id WHERE p.quantity<>COALESCE(b.quantity,0) OR p.inventory_value<>COALESCE(b.value,0)",
        ),
        (
            "stock_movement_quantities",
            "SELECT count(*) FROM products p LEFT JOIN (SELECT product_id,sum(quantity) quantity FROM inventory_movements GROUP BY product_id)m ON m.product_id=p.id WHERE p.quantity<>COALESCE(m.quantity,0)",
        ),
        (
            "stock_location_history",
            "SELECT count(*) FROM (SELECT product_id,warehouse_id,lot_id FROM stock_buckets UNION SELECT product_id,warehouse_id,lot_id FROM stock_location_events) k LEFT JOIN stock_buckets b ON b.product_id=k.product_id AND b.warehouse_id=k.warehouse_id AND b.lot_id IS NOT DISTINCT FROM k.lot_id LEFT JOIN LATERAL(SELECT COALESCE(sum(quantity),0) quantity,COALESCE(sum(value),0) value FROM stock_location_events e WHERE e.product_id=k.product_id AND e.warehouse_id=k.warehouse_id AND e.lot_id IS NOT DISTINCT FROM k.lot_id)e ON true WHERE COALESCE(b.quantity,0)<>e.quantity OR COALESCE(b.inventory_value,0)<>e.value",
        ),
        (
            "inventory_control",
            "SELECT CASE WHEN (SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE account_code='1300')=(SELECT COALESCE(sum(inventory_value),0) FROM products) THEN 0::bigint ELSE 1::bigint END",
        ),
        (
            "company_cardinality",
            "SELECT GREATEST(count(*)-1,0) FROM company",
        ),
        (
            "durable_storage",
            "SELECT count(*) FROM pg_settings WHERE name IN ('fsync','full_page_writes','synchronous_commit') AND setting='off'",
        ),
    ] {
        report.record(name, count(&mut tx, query).await?);
    }
    // Reuse the same event model as the party statements and MCP reports, including
    // opening balances, historical imports, allocations, refunds and reversals.
    let end = DateTime::from_timestamp(253_402_300_799, 999_999_000).expect("valid year 9999");
    for side in ["customer", "supplier"] {
        let events = crate::settlements::events_sql(side);
        let (debt, credits): (Decimal, Decimal) = sqlx::query_as(&format!("SELECT COALESCE(sum(debt_delta),0),COALESCE(sum(credit_delta),0) FROM ({events}) events"))
            .bind(Option::<Uuid>::None).bind(end).fetch_one(&mut *tx).await?;
        let obligation = crate::settlements::obligation_account(&mut tx, side)
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?;
        let control = crate::settlements::party_control_account(&mut tx, side)
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?;
        let sign = if side == "customer" { 1i32 } else { -1i32 };
        let ledger_debt: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(sum(debit-credit),0)*$2 FROM journal_lines WHERE account_code=$1",
        )
        .bind(obligation)
        .bind(sign)
        .fetch_one(&mut *tx)
        .await?;
        let ledger_credit: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(sum(debit-credit),0)*$2 FROM journal_lines WHERE account_code=$1",
        )
        .bind(control)
        .bind(-sign)
        .fetch_one(&mut *tx)
        .await?;
        report.record(
            if side == "customer" {
                "customer_control_reconciliation"
            } else {
                "supplier_control_reconciliation"
            },
            i64::from(ledger_debt != debt) + i64::from(ledger_credit != credits),
        );
    }
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE active")
        .fetch_one(&mut *tx)
        .await?;
    if active > 10 {
        report.warnings.push("Legacy installation has more than 10 active users; review inactive accounts before adding users. No account was changed.");
    }
    tx.rollback().await?;
    Ok(report)
}
