use axum::Router;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::supplier_bill_tests::{actor, call, fixture, request};

fn preview_body(csv: &str) -> Value {
    json!({"filename":"master.csv","csv":csv,"delimiter":"comma"})
}

async fn preview(app: &Router, cookie: &str, kind: &str, csv: &str) -> Value {
    call(
        app,
        cookie,
        "POST",
        &format!("/api/imports/{kind}/preview"),
        Some(preview_body(csv)),
    )
    .await
}

fn path(batch: &Value) -> String {
    format!("/api/imports/{}/commit", batch["id"].as_str().unwrap())
}

async fn commit(app: &Router, cookie: &str, batch: &Value) -> Value {
    call(
        app,
        cookie,
        "POST",
        &path(batch),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await
}

async fn master_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('customers',(SELECT count(*) FROM customers),'suppliers',(SELECT count(*) FROM suppliers),'products',(SELECT count(*) FROM products),'movements',(SELECT count(*) FROM inventory_movements),'journals',(SELECT count(*) FROM journal_entries),'value',(SELECT COALESCE(sum(inventory_value),0)::text FROM products),'quantity',(SELECT COALESCE(sum(quantity),0)::text FROM products))")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_preview_commit_all_masters_no_financial_postings_and_retry(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let original = master_snapshot(&pool).await;
    let customers = preview(&app,&cookie,"customers","name,email,phone,address\r\n\"Toko \"\"Maju\"\"\",toko@example.test,00123,\"Jalan A\nJakarta\"\r\n").await;
    assert_eq!(customers["status"], "ready");
    assert_eq!(customers["row_count"], 1);
    assert_eq!(customers["can_commit"], true);
    assert_eq!(master_snapshot(&pool).await, original);
    let input = json!({"idempotency_key":Uuid::new_v4()});
    let committed = call(
        &app,
        &cookie,
        "POST",
        &path(&customers),
        Some(input.clone()),
    )
    .await;
    assert_eq!(committed["status"], "committed");
    assert_eq!(committed["created_count"], 1);
    assert_eq!(committed["can_commit"], false);
    assert_eq!(
        call(
            &app,
            &cookie,
            "POST",
            &path(&customers),
            Some(input.clone())
        )
        .await,
        committed
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&customers),
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0,
        409
    );
    let suppliers = preview(&app, &cookie, "suppliers", "code,name\nsup-a,Supplier A\n").await;
    assert_eq!(suppliers["rows"][0]["data"]["code"], "SUP-A");
    commit(&app, &cookie, &suppliers).await;
    let products = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price,kind,unit\nstock-a,Barang A,12345.67,,\nservice-a,Jasa A,10,service,jam\n",
    )
    .await;
    commit(&app, &cookie, &products).await;
    let snapshot = master_snapshot(&pool).await;
    assert_eq!(snapshot["customers"], 1);
    assert_eq!(snapshot["suppliers"], 1);
    assert_eq!(snapshot["products"], 2);
    for field in ["journals", "movements"] {
        assert_eq!(snapshot[field], original[field]);
    }
    let balances: bool = sqlx::query_scalar(
        "SELECT bool_and(quantity=0 AND average_cost=0 AND inventory_value=0) FROM products",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(balances);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT active FROM suppliers")
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    let customer: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM customers c")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(customer["phone"], "00123");
    assert_eq!(customer["address"], "Jalan A\nJakarta");
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action IN ('customer.created','supplier.created','product.created') AND details ? 'import_id'").fetch_one(&pool).await.unwrap();
    assert_eq!(events, 4);
    let raw_csv: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit_events WHERE details ? 'csv')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!raw_csv);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_invalid_headers_rows_duplicates_and_limits_persist_without_writes(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let before = master_snapshot(&pool).await;
    for csv in [
        "name,name\na,b",
        "name,balance\nA,100",
        "email\na@example.test",
        "name\n\"unterminated",
    ] {
        let batch = preview(&app, &cookie, "customers", csv).await;
        assert_eq!(batch["status"], "invalid");
        assert_eq!(batch["can_commit"], false);
        assert!(!batch["errors"].as_array().unwrap().is_empty());
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                &path(&batch),
                Some(json!({"idempotency_key":Uuid::new_v4()}))
            )
            .await
            .0,
            409
        );
    }
    let batch = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\na,A,10\n A ,B,20\nb,B,0\nc,C,1.001\nd,D,1000000001\ne,E\nf,F,1,extra\n",
    )
    .await;
    assert_eq!(batch["status"], "invalid");
    assert_eq!(batch["row_count"], 7);
    assert_eq!(batch["valid_count"], 1);
    assert_eq!(batch["rows"][1]["errors"][0]["column"], "sku");
    assert_eq!(batch["rows"][1]["errors"][0]["row"], 3);
    let controls = preview(
        &app,
        &cookie,
        "customers",
        "name,email\nBad\0Name,bad\tmail\n",
    )
    .await;
    assert_eq!(controls["status"], "invalid");
    assert_eq!(controls["rows"][0]["data"]["name"], "Bad\\0Name");
    let nul_header = preview(&app, &cookie, "customers", "na\0me\nName").await;
    assert_eq!(nul_header["status"], "invalid");
    let too_many = format!("name\n{}", "Customer\n".repeat(1001));
    assert_eq!(
        preview(&app, &cookie, "customers", &too_many).await["status"],
        "invalid"
    );
    let huge = "x".repeat(256 * 1024 + 1);
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/imports/customers/preview",
            Some(preview_body(&huge))
        )
        .await
        .0,
        400
    );
    let exact_max = format!("name\nMax\n{}", "\n".repeat(256 * 1024 - 9));
    assert_eq!(exact_max.len(), 256 * 1024);
    let bounded = preview(&app, &cookie, "customers", &exact_max).await;
    assert_eq!(bounded["status"], "ready");
    assert_eq!(bounded["row_count"], 1);
    let mut bad = preview_body("name\nTest");
    bad["delimiter"] = json!("auto");
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/imports/customers/preview",
            Some(bad)
        )
        .await
        .0,
        400
    );
    let mut bad = preview_body("name\nTest");
    bad["filename"] = json!("bad\0.csv");
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/imports/customers/preview",
            Some(bad)
        )
        .await
        .0,
        400
    );
    assert_eq!(master_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_roles_actor_bound_commit_and_kind_filtered_history(pool: PgPool) {
    let (app, _, admin) = fixture(&pool).await;
    let (_, sales) = actor(&pool, "sales").await;
    let (_, warehouse) = actor(&pool, "warehouse").await;
    let (_, finance) = actor(&pool, "finance").await;
    let (_, other_sales) = actor(&pool, "sales").await;
    let batch = preview(&app, &sales, "customers", "name\nSales customer").await;
    let supplier = preview(
        &app,
        &warehouse,
        "suppliers",
        "code,name\nWARE,Warehouse supplier",
    )
    .await;
    for cookie in [&warehouse, &finance] {
        assert_eq!(
            request(
                &app,
                cookie,
                "POST",
                "/api/imports/customers/preview",
                Some(preview_body("name\nNo access"))
            )
            .await
            .0,
            403
        );
        assert_eq!(
            request(&app, cookie, "GET", "/api/imports/customers/template", None)
                .await
                .0,
            403
        );
        assert_eq!(
            request(
                &app,
                cookie,
                "GET",
                &format!("/api/imports/{}", batch["id"].as_str().unwrap()),
                None
            )
            .await
            .0,
            403
        );
    }
    for cookie in [&other_sales, &admin] {
        let detail = call(
            &app,
            cookie,
            "GET",
            &format!("/api/imports/{}", batch["id"].as_str().unwrap()),
            None,
        )
        .await;
        assert_eq!(detail["can_commit"], false);
        assert_eq!(
            request(
                &app,
                cookie,
                "POST",
                &path(&batch),
                Some(json!({"idempotency_key":Uuid::new_v4()}))
            )
            .await
            .0,
            403
        );
    }
    assert_eq!(
        request(&app, &sales, "GET", "/api/imports/products/template", None)
            .await
            .0,
        403
    );
    assert!(
        call(&app, &sales, "GET", "/api/imports/customers/template", None).await["raw"]
            .as_str()
            .unwrap()
            .starts_with('\u{feff}')
    );
    let template = call(
        &app,
        &warehouse,
        "GET",
        "/api/imports/suppliers/template?delimiter=semicolon",
        None,
    )
    .await;
    assert!(
        template["raw"]
            .as_str()
            .unwrap()
            .contains("code;name;email;phone;address")
    );
    let sales_history = call(&app, &sales, "GET", "/api/imports", None).await;
    assert_eq!(sales_history["items"].as_array().unwrap().len(), 1);
    assert_eq!(sales_history["items"][0]["id"], batch["id"]);
    let ware_history = call(&app, &warehouse, "GET", "/api/imports", None).await;
    assert_eq!(ware_history["items"].as_array().unwrap().len(), 1);
    assert_eq!(ware_history["items"][0]["id"], supplier["id"]);
    assert_eq!(
        request(&app, &finance, "GET", "/api/imports", None).await.0,
        403
    );
    assert_eq!(
        request(&app, &sales, "GET", "/api/imports?kind=suppliers", None)
            .await
            .0,
        403
    );
    let committed = commit(&app, &sales, &batch).await;
    assert_eq!(committed["status"], "committed");
    let key: Uuid = sqlx::query_scalar("SELECT commit_key FROM master_import_batches WHERE id=$1")
        .bind(batch["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            &admin,
            "POST",
            &path(&batch),
            Some(json!({"idempotency_key":key}))
        )
        .await
        .0,
        403
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_stale_duplicate_revalidation_is_atomic_and_customer_names_normalized(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let customer = preview(
        &app,
        &cookie,
        "customers",
        "name\n New customer \nexisting customer\n",
    )
    .await;
    call(
        &app,
        &cookie,
        "POST",
        "/api/customers",
        Some(json!({"name":"  EXISTING CUSTOMER  "})),
    )
    .await;
    let before = master_snapshot(&pool).await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&customer),
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        )
        .await
        .0,
        409
    );
    assert_eq!(master_snapshot(&pool).await, before);
    let product = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\nNEW,New product,1\nEXIST,Existing product,2\n",
    )
    .await;
    // Also handle a legacy lowercase identity that predates normalization.
    sqlx::query("INSERT INTO products(id,sku,name,price) VALUES($1,'exist','Legacy product',3)")
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();
    let before = master_snapshot(&pool).await;
    let (status, body) = request(
        &app,
        &cookie,
        "POST",
        &path(&product),
        Some(json!({"idempotency_key":Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Baris 3")
    );
    assert_eq!(master_snapshot(&pool).await, before);
    let rejected = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\n ExIsT ,Same identity,1\n",
    )
    .await;
    assert_eq!(rejected["status"], "invalid");
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_concurrent_retry_and_competing_previews_create_once(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let batch = preview(
        &app,
        &cookie,
        "suppliers",
        "code,name\nRETRY,Supplier Retry",
    )
    .await;
    let input = json!({"idempotency_key":Uuid::new_v4()});
    let path = path(&batch);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", &path, Some(input.clone())),
        request(&app, &cookie, "POST", &path, Some(input.clone()))
    );
    assert!(a.0.is_success(), "{a:?}");
    assert_eq!(a, b);
    let one = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\nRACE,Race product,1",
    )
    .await;
    let two = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\nrace,Other race product,2",
    )
    .await;
    let one_path = self::path(&one);
    let two_path = self::path(&two);
    let (a, b) = tokio::join!(
        request(
            &app,
            &cookie,
            "POST",
            &one_path,
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        ),
        request(
            &app,
            &cookie,
            "POST",
            &two_path,
            Some(json!({"idempotency_key":Uuid::new_v4()}))
        )
    );
    assert!(
        (a.0.is_success() && b.0 == 409) || (b.0.is_success() && a.0 == 409),
        "{a:?} {b:?}"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM products WHERE sku='RACE'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_imports_table_lock_rechecks_manual_create_and_batch_history_is_immutable(
    pool: PgPool,
) {
    let (app, _, cookie) = fixture(&pool).await;
    let batch = preview(
        &app,
        &cookie,
        "products",
        "sku,name,price\nLOCKED,Imported product,1",
    )
    .await;
    let mut manual = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO products(id,sku,name,price) VALUES($1,'LOCKED','Concurrent manual product',2)",
    )
    .bind(Uuid::new_v4())
    .execute(&mut *manual)
    .await
    .unwrap();
    let app_copy = app.clone();
    let cookie_copy = cookie.clone();
    let path = path(&batch);
    let commit_task = tokio::spawn(async move {
        request(
            &app_copy,
            &cookie_copy,
            "POST",
            &path,
            Some(json!({"idempotency_key":Uuid::new_v4()})),
        )
        .await
    });
    // The insertion holds RowExclusiveLock until commit; import must wait and then
    // observe that identity instead of inserting against its old preview.
    tokio::task::yield_now().await;
    manual.commit().await.unwrap();
    assert_eq!(commit_task.await.unwrap().0, 409);
    let valid = preview(&app, &cookie, "customers", "name\nImmutable history").await;
    let id: Uuid = valid["id"].as_str().unwrap().parse().unwrap();
    assert!(
        sqlx::query("UPDATE master_import_batches SET filename='changed.csv' WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM master_import_batches WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("TRUNCATE master_import_batches")
            .execute(&pool)
            .await
            .is_err()
    );
    let committed = commit(&app, &cookie, &valid).await;
    assert!(sqlx::query("UPDATE master_import_batches SET status='ready',committed_at=NULL,created_count=0,commit_key=NULL WHERE id=$1").bind(id).execute(&pool).await.is_err());
    let detail = call(&app, &cookie, "GET", &format!("/api/imports/{id}"), None).await;
    assert_eq!(detail, committed);
    assert_eq!(detail["hash"], valid["hash"]);
    assert_eq!(detail["rows"], valid["rows"]);
    assert_eq!(detail["errors"], valid["errors"]);
}
