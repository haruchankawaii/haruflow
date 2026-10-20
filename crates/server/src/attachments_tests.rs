use crate::supplier_bill_tests::{actor, call, fixture, request};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PDF: &[u8] = b"%PDF-1.7\n1 0 obj <<>> endobj\n%%EOF\n";
fn encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'-' | b'_') {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
async fn raw(
    app: &Router,
    cookie: &str,
    path: &str,
    media_type: &str,
    bytes: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("cookie", cookie)
                .header("x-haruflow-request", "1")
                .header("content-type", media_type)
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}
async fn get(app: &Router, cookie: &str, path: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}
fn upload_path(kind: &str, id: Uuid, filename: &str, key: Uuid) -> String {
    format!(
        "/api/documents/{kind}/{id}/attachments?filename={}&idempotency_key={key}",
        encode(filename)
    )
}
async fn journal(pool: &PgPool) -> (Router, String, Uuid) {
    let (app, _, cookie) = fixture(pool).await;
    let result=call(&app,&cookie,"POST","/api/accounting/journals",Some(json!({"description":"Lampiran bukti saldo awal","lines":[{"account_code":"1100","debit":"100","credit":"0"},{"account_code":"3000","debit":"0","credit":"100"}],"idempotency_key":Uuid::new_v4()}))).await;
    let id: Uuid = result["id"].as_str().unwrap().parse().unwrap();
    (app, cookie, id)
}
async fn ledger_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM journal_entries j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM journal_lines l),'products',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM products p),'movements',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM inventory_movements m))").fetch_one(pool).await.unwrap()
}
async fn upload(app: &Router, cookie: &str, kind: &str, id: Uuid, filename: &str) -> Value {
    let (status, _, bytes) = raw(
        app,
        cookie,
        &upload_path(kind, id, filename, Uuid::new_v4()),
        "application/pdf",
        PDF.to_vec(),
    )
    .await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}
fn transition(item: &Value, reason: &str) -> Value {
    json!({"version":item["version"],"reason":reason,"idempotency_key":Uuid::new_v4()})
}
fn path(item: &Value, action: &str) -> String {
    format!("/api/attachments/{}/{action}", item["id"].as_str().unwrap())
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_upload_unicode_download_archive_restore_and_ledger_unchanged(pool: PgPool) {
    let (app, cookie, id) = journal(&pool).await;
    let before = ledger_snapshot(&pool).await;
    let name = "Bukti pembayaran 日本語 2026.pdf";
    let item = upload(&app, &cookie, "journal", id, name).await;
    assert_eq!(item["filename"], name);
    assert_eq!(item["version"], 1);
    assert_eq!(item["byte_size"], PDF.len());
    assert_eq!(item["sha256"], hex::encode(Sha256::digest(PDF)));
    assert!(item.get("content").is_none());
    assert_eq!(item["history"][0]["action"], "uploaded");
    let download = path(&item, "download");
    let (status, headers, bytes) = get(&app, &cookie, &download).await;
    assert_eq!(status, 200);
    assert_eq!(bytes, PDF);
    assert_eq!(headers["content-type"], "application/pdf");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let disposition = headers["content-disposition"].to_str().unwrap();
    assert!(disposition.starts_with("attachment; filename=\""));
    assert!(
        disposition.contains(
            "filename*=UTF-8''Bukti%20pembayaran%20%E6%97%A5%E6%9C%AC%E8%AA%9E%202026.pdf"
        )
    );
    assert_eq!(get(&app, "", &download).await.0, 401);
    let archived = call(
        &app,
        &cookie,
        "POST",
        &path(&item, "archive"),
        Some(transition(&item, "Bukti lama disimpan sebagai arsip")),
    )
    .await;
    assert_eq!(archived["version"], 2);
    assert!(archived["archived_at"].is_string());
    assert_eq!(get(&app, &cookie, &download).await.2, PDF);
    let listed = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/documents/journal/{id}/attachments"),
        None,
    )
    .await;
    assert_eq!(listed["active_count"], 0);
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    let restored = call(
        &app,
        &cookie,
        "POST",
        &path(&item, "restore"),
        Some(transition(
            &archived,
            "Bukti diperlukan kembali untuk pemeriksaan",
        )),
    )
    .await;
    assert_eq!(restored["version"], 3);
    assert!(restored["archived_at"].is_null());
    assert_eq!(restored["history"].as_array().unwrap().len(), 3);
    assert_eq!(
        restored["history"][1]["reason"],
        "Bukti lama disimpan sebagai arsip"
    );
    assert_eq!(ledger_snapshot(&pool).await, before);
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE resource_type='attachment'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(events, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_enforce_document_roles_authenticated_reads_and_missing_targets(pool: PgPool) {
    let (app, admin, journal_id) = journal(&pool).await;
    let (_, sales) = actor(&pool, "sales").await;
    let (_, warehouse) = actor(&pool, "warehouse").await;
    let (_, finance) = actor(&pool, "finance").await;
    assert_eq!(
        raw(
            &app,
            &sales,
            &upload_path("journal", journal_id, "bukti.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        403
    );
    assert_eq!(
        raw(
            &app,
            &warehouse,
            &upload_path("journal", journal_id, "bukti.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        403
    );
    let item = upload(&app, &finance, "journal", journal_id, "bukti.pdf").await;
    assert_eq!(get(&app, &sales, &path(&item, "download")).await.0, 200);
    assert_eq!(
        request(
            &app,
            &warehouse,
            "POST",
            &path(&item, "archive"),
            Some(transition(&item, "Tidak boleh oleh gudang"))
        )
        .await
        .0,
        403
    );
    let customer = call(
        &app,
        &admin,
        "POST",
        "/api/customers",
        Some(json!({"name":"Pelanggan lampiran"})),
    )
    .await;
    let order_id = Uuid::new_v4();
    let creator: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sales_orders(id,number,customer_id,total,created_by) VALUES($1,'SO-ATTACHMENT',$2,1,$3)").bind(order_id).bind(customer["id"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(creator).execute(&pool).await.unwrap();
    assert_eq!(
        raw(
            &app,
            &finance,
            &upload_path("sales_order", order_id, "bukti.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        403
    );
    upload(&app, &sales, "sales_order", order_id, "bukti.pdf").await;
    assert_eq!(
        raw(
            &app,
            &admin,
            &upload_path("journal", Uuid::new_v4(), "bukti.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &app,
            &admin,
            &upload_path("customer", journal_id, "bukti.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        404
    );
    assert_eq!(
        get(
            &app,
            "",
            &format!("/api/documents/journal/{journal_id}/attachments")
        )
        .await
        .0,
        401
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_validate_type_filename_size_and_limit_override_only_upload(pool: PgPool) {
    let (app, cookie, id) = journal(&pool).await;
    for name in [
        "",
        ".",
        "..",
        "../invoice.pdf",
        "a\\b.pdf",
        "a\nb.pdf",
        " a.pdf",
        "a.pdf ",
        "ab\u{202e}fdp.exe",
    ] {
        assert_eq!(
            raw(
                &app,
                &cookie,
                &upload_path("journal", id, name, Uuid::new_v4()),
                "application/pdf",
                PDF.to_vec()
            )
            .await
            .0,
            400,
            "{name:?}"
        );
    }
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, &"x".repeat(201), Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        400
    );
    for (media_type, bytes) in [
        ("image/png", PDF.to_vec()),
        ("application/pdf", b"not a pdf".to_vec()),
        ("text/html", b"<html>evil</html>".to_vec()),
        ("application/pdf", vec![]),
        ("application/pdf", b"%PDF-1.7 no end".to_vec()),
        ("image/jpeg", b"\xff\xd8\xffmissing end".to_vec()),
    ] {
        assert_eq!(
            raw(
                &app,
                &cookie,
                &upload_path("journal", id, "bukti.pdf", Uuid::new_v4()),
                media_type,
                bytes
            )
            .await
            .0,
            400
        );
    }
    let mut exact = vec![b' '; crate::attachments::MAX_FILE_BYTES];
    exact[..9].copy_from_slice(b"%PDF-1.7\n");
    let end = exact.len() - 6;
    exact[end..].copy_from_slice(b"%%EOF\n");
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "large.pdf", Uuid::new_v4()),
            "application/pdf",
            exact.clone()
        )
        .await
        .0,
        200
    );
    exact.push(b' ');
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "too-large.pdf", Uuid::new_v4()),
            "application/pdf",
            exact
        )
        .await
        .0,
        413
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            "/api/accounting/journals",
            Some(json!({"description":"x".repeat(300_000)}))
        )
        .await
        .0,
        413
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document_attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let png=hex::decode("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c636000020000050001a5f645400000000049454e44ae426082").unwrap();
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "image.png", Uuid::new_v4()),
            "image/png",
            png
        )
        .await
        .0,
        200
    );
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "image.jpg", Uuid::new_v4()),
            "image/jpeg",
            b"\xff\xd8\xff\xe0\0\x02\xff\xd9".to_vec()
        )
        .await
        .0,
        200
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_idempotency_concurrency_conflicts_and_stale_versions(pool: PgPool) {
    let (app, cookie, id) = journal(&pool).await;
    let key = Uuid::new_v4();
    let url = upload_path("journal", id, "retry.pdf", key);
    let (a, b) = tokio::join!(
        raw(&app, &cookie, &url, "application/pdf", PDF.to_vec()),
        raw(&app, &cookie, &url, "application/pdf", PDF.to_vec())
    );
    assert_eq!(a.0, 200);
    assert_eq!(a.2, b.2);
    let item: Value = serde_json::from_slice(&a.2).unwrap();
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "changed.pdf", key),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        409
    );
    let (_, other) = actor(&pool, "finance").await;
    assert_eq!(
        raw(&app, &other, &url, "application/pdf", PDF.to_vec())
            .await
            .0,
        409
    );
    let body = transition(&item, "Lampiran diarsipkan untuk koreksi");
    let archived = call(
        &app,
        &cookie,
        "POST",
        &path(&item, "archive"),
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        call(&app, &cookie, "POST", &path(&item, "archive"), Some(body)).await,
        archived
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&item, "restore"),
            Some(transition(&item, "Versi lama harus ditolak"))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&item, "restore"),
            Some(transition(&archived, "x"))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&item, "restore"),
            Some(transition(&archived, "Kontrol\nterlarang"))
        )
        .await
        .0,
        400
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document_attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document_attachment_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_count_quota_serializes_uploads_and_restore(pool: PgPool) {
    let (app, cookie, id) = journal(&pool).await;
    let first = upload(&app, &cookie, "journal", id, "first.pdf").await;
    for n in 1..19 {
        upload(&app, &cookie, "journal", id, &format!("bukti-{n}.pdf")).await;
    }
    let a = upload_path("journal", id, "competing-a.pdf", Uuid::new_v4());
    let b = upload_path("journal", id, "competing-b.pdf", Uuid::new_v4());
    let (a, b) = tokio::join!(
        raw(&app, &cookie, &a, "application/pdf", PDF.to_vec()),
        raw(&app, &cookie, &b, "application/pdf", PDF.to_vec())
    );
    assert!(matches!(
        (a.0.as_u16(), b.0.as_u16()),
        (200, 409) | (409, 200)
    ));
    let archived = call(
        &app,
        &cookie,
        "POST",
        &path(&first, "archive"),
        Some(transition(&first, "Menyimpan versi lama bukti")),
    )
    .await;
    upload(&app, &cookie, "journal", id, "replacement.pdf").await;
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&first, "restore"),
            Some(transition(&archived, "Pemulihan melebihi kuota"))
        )
        .await
        .0,
        409
    );
    let listed = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/documents/journal/{id}/attachments"),
        None,
    )
    .await;
    assert_eq!(listed["active_count"], 20);
    assert_eq!(listed["items"].as_array().unwrap().len(), 21);
}

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_total_bytes_quota_and_database_immutability(pool: PgPool) {
    let (app, cookie, id) = journal(&pool).await;
    let item = upload(&app, &cookie, "journal", id, "original.pdf").await;
    let attachment_id: Uuid = item["id"].as_str().unwrap().parse().unwrap();
    assert!(sqlx::query("UPDATE document_attachments SET content=$2,sha256=encode(sha256($2),'hex'),version=version+1,archived_at=now(),archived_by=created_by,archive_reason='Invalid content edit' WHERE id=$1").bind(attachment_id).bind(b"changed".to_vec()).execute(&pool).await.is_err());
    assert!(
        sqlx::query("DELETE FROM document_attachments WHERE id=$1")
            .bind(attachment_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE document_attachment_events SET reason='Tamper event'")
            .execute(&pool)
            .await
            .is_err()
    );
    let archived = call(
        &app,
        &cookie,
        "POST",
        &path(&item, "archive"),
        Some(transition(&item, "Arsip sebelum batas byte diuji")),
    )
    .await;
    let mut exact = vec![b' '; crate::attachments::MAX_FILE_BYTES];
    exact[..9].copy_from_slice(b"%PDF-1.7\n");
    let end = exact.len() - 6;
    exact[end..].copy_from_slice(b"%%EOF\n");
    for n in 0..10 {
        let (status, _, body) = raw(
            &app,
            &cookie,
            &upload_path("journal", id, &format!("large-{n}.pdf"), Uuid::new_v4()),
            "application/pdf",
            exact.clone(),
        )
        .await;
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    }
    assert_eq!(
        raw(
            &app,
            &cookie,
            &upload_path("journal", id, "one-too-many.pdf", Uuid::new_v4()),
            "application/pdf",
            PDF.to_vec()
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "POST",
            &path(&item, "restore"),
            Some(transition(&archived, "Pulihkan melewati batas byte"))
        )
        .await
        .0,
        409
    );
    let listed = call(
        &app,
        &cookie,
        "GET",
        &format!("/api/documents/journal/{id}/attachments"),
        None,
    )
    .await;
    assert_eq!(listed["active_count"], 10);
    assert_eq!(listed["active_bytes"], 52_428_800);
    assert_eq!(get(&app, &cookie, &path(&item, "download")).await.2, PDF);
}
