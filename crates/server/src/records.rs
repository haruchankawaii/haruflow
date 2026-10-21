use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    routing::get,
};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/records/{kind}", get(list))
        .route("/records/{kind}/export", get(export))
}

#[derive(Default, Deserialize)]
struct Filters {
    q: Option<String>,
    status: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    page: Option<u32>,
    page_size: Option<u32>,
}
struct Source {
    sql: &'static str,
    own_only: bool,
}
fn source(kind: &str, user: &AuthUser) -> AppResult<Source> {
    let own_only = kind == "imports" && user.role != "admin";
    if kind == "audit" {
        user.require(&["finance"])?;
    }
    let sql = match kind {
        "customers" => {
            "SELECT id,name label,COALESCE(email,'') party,NULL::numeric amount,'active'::text status,created_at,NULL::text url,name||' '||COALESCE(email,'')||' '||COALESCE(phone,'') search,NULL::uuid owner_id FROM customers"
        }
        "suppliers" => {
            "SELECT id,code||' · '||name label,COALESCE(email,'') party,NULL::numeric amount,CASE WHEN active THEN 'active' ELSE 'inactive' END status,created_at,NULL::text url,code||' '||name||' '||COALESCE(email,'') search,NULL::uuid owner_id FROM suppliers"
        }
        "products" => {
            "SELECT id,sku||' · '||name label,unit party,price amount,kind status,created_at,NULL::text url,sku||' '||name search,NULL::uuid owner_id FROM products"
        }
        "sales-orders" => {
            "SELECT o.id,o.number label,c.name party,o.total amount,o.status,o.created_at,'?order='||o.id||'#sales' url,o.number||' '||c.name search,NULL::uuid owner_id FROM sales_orders o JOIN customers c ON c.id=o.customer_id"
        }
        "invoices" => {
            "SELECT i.id,i.number label,c.name party,i.total-i.credit_total-i.paid amount,i.status,i.created_at,'?invoice='||i.id||'#sales' url,i.number||' '||c.name search,NULL::uuid owner_id FROM invoices i JOIN customers c ON c.id=i.customer_id"
        }
        "purchase-orders" => {
            "SELECT id,number label,supplier_name party,total amount,status,created_at,'?purchase='||id||'#purchasing' url,number||' '||supplier_name search,NULL::uuid owner_id FROM purchase_orders"
        }
        "purchase-bills" => {
            "SELECT id,number||' · '||supplier_invoice_number label,supplier_name party,total-credit_total-paid amount,status,created_at,'?bill='||id||'#purchasing' url,number||' '||supplier_name||' '||supplier_invoice_number search,NULL::uuid owner_id FROM purchase_bills"
        }
        "inventory-movements" => {
            "SELECT m.id,p.sku||' · '||COALESCE(m.reference,m.kind) label,p.name party,m.value amount,m.kind status,m.created_at,NULL::text url,p.sku||' '||p.name||' '||COALESCE(m.reference,'') search,NULL::uuid owner_id FROM inventory_movements m JOIN products p ON p.id=m.product_id"
        }
        "journals" => {
            "SELECT id,description label,source_type party,NULL::numeric amount,source_type status,created_at,NULL::text url,description||' '||source_type||' '||source_id search,NULL::uuid owner_id FROM journal_entries"
        }
        "audit" => {
            "SELECT e.id,e.action label,COALESCE(u.name,'') party,NULL::numeric amount,e.resource_type status,e.created_at,NULL::text url,e.action||' '||e.resource_type||' '||COALESCE(u.name,'')||' '||COALESCE(e.resource_id::text,'') search,NULL::uuid owner_id FROM audit_events e LEFT JOIN users u ON u.id=e.actor_id"
        }
        "imports" => {
            "SELECT id,filename label,kind party,created_count::numeric amount,status,created_at,'?import='||kind||'#settings' url,filename||' '||kind search,created_by owner_id FROM master_import_batches"
        }
        "party-funds" => {
            "SELECT f.id,f.number label,COALESCE(c.name,s.name,'') party,b.available amount,f.kind status,f.created_at,'?side='||f.side||'&party='||f.party_id||'#balances' url,f.number||' '||COALESCE(c.name,s.name,'')||' '||COALESCE(f.reference,'') search,NULL::uuid owner_id FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b LEFT JOIN customers c ON c.id=f.customer_id LEFT JOIN suppliers s ON s.id=f.supplier_id"
        }
        _ => return Err(AppError::not_found()),
    };
    Ok(Source { sql, own_only })
}
fn pattern(value: Option<&str>) -> AppResult<Option<String>> {
    value
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            if s.chars().count() > 200 || s.chars().any(char::is_control) {
                return Err(AppError::bad_request(
                    "Pencarian maksimal 200 karakter tanpa karakter kontrol.",
                ));
            }
            Ok(format!(
                "%{}%",
                s.trim()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            ))
        })
        .transpose()
}
fn validate(f: &Filters) -> AppResult<(u32, u32)> {
    let page = f.page.unwrap_or(1);
    let size = f.page_size.unwrap_or(50);
    if !(1..=100000).contains(&page) || !(1..=100).contains(&size) {
        return Err(AppError::bad_request(
            "Halaman harus 1–100000 dan ukuran halaman 1–100.",
        ));
    }
    if f.from.zip(f.to).is_some_and(|(a, b)| a > b)
        || [f.from, f.to]
            .into_iter()
            .flatten()
            .any(|date| !(1..=9999).contains(&date.year()))
    {
        return Err(AppError::bad_request("Rentang tanggal tidak valid."));
    }
    if f.status
        .as_ref()
        .is_some_and(|s| s.len() > 80 || s.chars().any(char::is_control))
    {
        return Err(AppError::bad_request("Filter status tidak valid."));
    }
    Ok((page, size))
}
const WHERE: &str = r#" WHERE ($1::text IS NULL OR search ILIKE $1 ESCAPE E'\\') AND ($2::text IS NULL OR status=$2)
AND ($3::date IS NULL OR (created_at AT TIME ZONE (SELECT timezone FROM company))::date >= $3)
AND ($4::date IS NULL OR (created_at AT TIME ZONE (SELECT timezone FROM company))::date <= $4)
AND ($5::uuid IS NULL OR owner_id=$5)"#;
async fn read(
    state: &AppState,
    user: &AuthUser,
    kind: &str,
    f: &Filters,
    is_export: bool,
) -> AppResult<Value> {
    let (page, size) = validate(f)?;
    let source = source(kind, user)?;
    let q = pattern(f.q.as_deref())?;
    let status = f.status.as_deref().filter(|s| !s.is_empty());
    let owner = source.own_only.then_some(user.id);
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let count_sql = format!("SELECT count(*) FROM ({}) records{WHERE}", source.sql);
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(&q)
        .bind(status)
        .bind(f.from)
        .bind(f.to)
        .bind(owner)
        .fetch_one(&mut *tx)
        .await?;
    if is_export && total > 10000 {
        return Err(AppError::conflict(
            "Hasil ekspor melebihi 10.000 baris. Persempit pencarian atau rentang tanggal; tidak ada data yang diekspor sebagian.",
        ));
    }
    let sql = format!(
        "SELECT id,label,party,amount::text,status,created_at,url FROM ({}) records{WHERE} ORDER BY created_at DESC,id DESC LIMIT $6 OFFSET $7",
        source.sql
    );
    let limit = if is_export { 10000 } else { i64::from(size) };
    let offset = if is_export {
        0
    } else {
        i64::from(page - 1) * i64::from(size)
    };
    let rows = sqlx::query(&sql)
        .bind(&q)
        .bind(status)
        .bind(f.from)
        .bind(f.to)
        .bind(owner)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tx)
        .await?;
    let items:Vec<Value>=rows.iter().map(|row|json!({"id":row.get::<Uuid,_>("id"),"label":row.get::<String,_>("label"),"party":row.get::<String,_>("party"),"amount":row.get::<Option<String>,_>("amount"),"status":row.get::<String,_>("status"),"created_at":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"url":row.get::<Option<String>,_>("url")})).collect();
    tx.commit().await?;
    Ok(
        json!({"items":items,"pagination":{"page":page,"page_size":size,"total":total,"pages":((total+i64::from(size)-1)/i64::from(size)).max(1)}}),
    )
}
async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(kind): Path<String>,
    Query(f): Query<Filters>,
) -> AppResult<Json<Value>> {
    Ok(Json(read(&state, &user, &kind, &f, false).await?))
}
fn csv_cell(value: &str) -> String {
    let dangerous = value.trim_start().starts_with(['=', '+', '-', '@']);
    format!(
        "\"{}{}\"",
        if dangerous { "'" } else { "" },
        value.replace('"', "\"\"")
    )
}
async fn export(
    State(state): State<AppState>,
    user: AuthUser,
    Path(kind): Path<String>,
    Query(f): Query<Filters>,
) -> AppResult<(HeaderMap, String)> {
    let data = read(&state, &user, &kind, &f, true).await?;
    let mut csv = String::from("\u{feff}id,tanggal,dokumen,pihak,nominal,status\r\n");
    for row in data["items"].as_array().unwrap() {
        csv.push_str(
            &["id", "created_at", "label", "party", "amount", "status"]
                .map(|key| csv_cell(row[key].as_str().unwrap_or("")))
                .join(","),
        );
        csv.push_str("\r\n");
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"haruflow-{kind}.csv\""))
            .map_err(|e| AppError::internal(e.into()))?,
    );
    Ok((headers, csv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supplier_bill_tests::{actor, call, fixture, request};
    #[test]
    fn literal_search_and_csv_formula_protection() {
        assert_eq!(pattern(Some("%_\\")).unwrap(), Some("%\\%\\_\\\\%".into()));
        assert_eq!(csv_cell(" =1+1"), "\"' =1+1\"");
        assert_eq!(csv_cell("A,\"B\""), "\"A,\"\"B\"\"\"");
        assert!(
            validate(&Filters {
                page_size: Some(101),
                ..Default::default()
            })
            .is_err()
        );
    }
    #[sqlx::test(migrations = "../../migrations")]
    async fn history_search_reaches_old_rows_and_pages_are_complete_and_role_scoped(
        pool: sqlx::PgPool,
    ) {
        let (app, _, cookie) = fixture(&pool).await;
        sqlx::query("INSERT INTO customers(id,name,created_at) SELECT md5('paged-customer-'||n)::uuid,'Customer '||lpad(n::text,4,'0'),'2025-01-01'::timestamptz FROM generate_series(1,1207)n").execute(&pool).await.unwrap();
        let old = call(
            &app,
            &cookie,
            "GET",
            "/api/records/customers?q=Customer%200001",
            None,
        )
        .await;
        assert_eq!(old["pagination"]["total"], 1);
        assert_eq!(old["items"][0]["label"], "Customer 0001");
        let mut ids = std::collections::BTreeSet::new();
        for page in 1..=13 {
            let result = call(
                &app,
                &cookie,
                "GET",
                &format!("/api/records/customers?page={page}&page_size=100"),
                None,
            )
            .await;
            assert_eq!(result["pagination"]["total"], 1207);
            for row in result["items"].as_array().unwrap() {
                assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
            }
        }
        assert_eq!(ids.len(), 1207);
        let (status, csv) = request(
            &app,
            &cookie,
            "GET",
            "/api/records/customers/export?q=Customer",
            None,
        )
        .await;
        assert!(status.is_success());
        assert_eq!(csv["raw"].as_str().unwrap().lines().count(), 1208);
        let (_, viewer) = actor(&pool, "viewer").await;
        assert_eq!(
            request(&app, &viewer, "GET", "/api/records/audit", None)
                .await
                .0,
            403
        );
        assert_eq!(
            request(&app, "", "GET", "/api/records/customers", None)
                .await
                .0,
            401
        );
        assert_eq!(
            request(
                &app,
                &cookie,
                "GET",
                "/api/records/customers?page_size=101",
                None
            )
            .await
            .0,
            400
        );
        assert_eq!(
            call(
                &app,
                &cookie,
                "GET",
                "/api/records/customers?from=2026-01-01",
                None
            )
            .await["pagination"]["total"],
            0
        );
        for kind in [
            "suppliers",
            "products",
            "sales-orders",
            "invoices",
            "purchase-orders",
            "purchase-bills",
            "inventory-movements",
            "journals",
            "audit",
            "imports",
            "party-funds",
        ] {
            assert!(
                call(&app, &cookie, "GET", &format!("/api/records/{kind}"), None).await["items"]
                    .is_array(),
                "{kind}"
            );
        }
        sqlx::query("INSERT INTO customers(id,name) SELECT md5('overflow-customer-'||n)::uuid,'Overflow customer '||n FROM generate_series(1,9000)n").execute(&pool).await.unwrap();
        assert_eq!(
            request(&app, &cookie, "GET", "/api/records/customers/export", None)
                .await
                .0,
            409,
            "An oversized export must refuse instead of silently truncating"
        );
        assert!(
            request(
                &app,
                &cookie,
                "GET",
                "/api/records/customers/export?q=Customer%200001",
                None
            )
            .await
            .0
            .is_success()
        );
    }
}
