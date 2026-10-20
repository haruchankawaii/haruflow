use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, payload, remember},
    error::{AppError, AppResult},
    supplier_bills::clean_text,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(crate) const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
const MAX_FILES: i64 = 20;
const MAX_DOCUMENT_BYTES: i64 = 50 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/documents/{kind}/{id}/attachments",
            get(list)
                .post(upload)
                .layer(DefaultBodyLimit::max(MAX_FILE_BYTES)),
        )
        .route("/attachments/{id}/download", get(download))
        .route("/attachments/{id}/archive", post(archive))
        .route("/attachments/{id}/restore", post(restore))
}

fn target(kind: &str) -> AppResult<(&'static str, &'static str)> {
    match kind {
        "sales_order" => Ok(("sales_orders", "sales")),
        "invoice" => Ok(("invoices", "finance")),
        "payment" => Ok(("payments", "finance")),
        "purchase_order" => Ok(("purchase_orders", "warehouse")),
        "purchase_receipt" => Ok(("purchase_receipts", "warehouse")),
        "purchase_bill" => Ok(("purchase_bills", "finance")),
        "supplier_payment" => Ok(("supplier_payments", "finance")),
        "journal" => Ok(("journal_entries", "finance")),
        "sales_return" => Ok(("sales_returns", "finance")),
        "supplier_return" => Ok(("supplier_returns", "finance")),
        _ => Err(AppError::not_found()),
    }
}

async fn target_exists(tx: &mut Transaction<'_, Postgres>, kind: &str, id: Uuid) -> AppResult<()> {
    let (table, _) = target(kind)?;
    // The table name comes exclusively from the static allowlist above.
    let exists: bool =
        sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=$1)"))
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::not_found())
    }
}

async fn lock_document(tx: &mut Transaction<'_, Postgres>, kind: &str, id: Uuid) -> AppResult<()> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('attachment:'||$1||':'||$2::uuid::text,0))",
    )
    .bind(kind)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn check_quota(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    id: Uuid,
    size: i32,
) -> AppResult<()> {
    let row = sqlx::query("SELECT count(*) AS count,COALESCE(sum(byte_size),0)::bigint AS bytes FROM document_attachments WHERE document_type=$1 AND document_id=$2 AND archived_at IS NULL")
        .bind(kind).bind(id).fetch_one(&mut **tx).await?;
    if row.get::<i64, _>("count") >= MAX_FILES
        || row.get::<i64, _>("bytes") + i64::from(size) > MAX_DOCUMENT_BYTES
    {
        return Err(AppError::conflict(
            "Batas lampiran aktif tercapai: maksimal 20 file dan 50 MiB per dokumen. Arsipkan lampiran yang tidak diperlukan sebelum menambahkan atau memulihkan file.",
        ));
    }
    Ok(())
}

const METADATA: &str = r#"SELECT jsonb_build_object(
    'id',a.id,'document_type',a.document_type,'document_id',a.document_id,
    'filename',a.filename,'media_type',a.media_type,'byte_size',a.byte_size,'sha256',a.sha256,
    'version',a.version,'created_at',a.created_at,'created_by_name',u.name,
    'archived_at',a.archived_at,'archived_by_name',v.name,'archive_reason',a.archive_reason,
    'history',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',e.id,'action',e.action,'reason',e.reason,
      'version',e.version,'created_at',e.created_at,'created_by_name',eu.name) ORDER BY e.version)
      FROM document_attachment_events e JOIN users eu ON eu.id=e.created_by WHERE e.attachment_id=a.id),'[]'::jsonb))
    FROM document_attachments a JOIN users u ON u.id=a.created_by LEFT JOIN users v ON v.id=a.archived_by"#;

async fn metadata(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Value> {
    sqlx::query_scalar(&format!("{METADATA} WHERE a.id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(AppError::not_found)
}

async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    target_exists(&mut tx, &kind, id).await?;
    let items: Vec<Value> = sqlx::query_scalar(&format!(
        "{METADATA} WHERE a.document_type=$1 AND a.document_id=$2 ORDER BY a.created_at,a.id"
    ))
    .bind(&kind)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let active: Vec<_> = items
        .iter()
        .filter(|item| item["archived_at"].is_null())
        .collect();
    let count = active.len();
    let bytes: i64 = active
        .iter()
        .map(|item| item["byte_size"].as_i64().unwrap_or(0))
        .sum();
    tx.commit().await?;
    Ok(Json(
        json!({"items":items,"limits":{"max_file_bytes":MAX_FILE_BYTES,"max_files":MAX_FILES,"max_document_bytes":MAX_DOCUMENT_BYTES},"active_count":count,"active_bytes":bytes}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadQuery {
    filename: String,
    idempotency_key: Uuid,
}

fn filename(value: &str) -> AppResult<String> {
    if value.is_empty()
        || value.chars().count() > 200
        || value.trim() != value
        || matches!(value, "." | "..")
        || value.chars().any(|c| {
            c.is_control() || matches!(c,'/'|'\\'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        })
    {
        return Err(AppError::bad_request(
            "Nama file harus 1–200 karakter tanpa pemisah folder, karakter kontrol, atau spasi di tepinya.",
        ));
    }
    Ok(value.to_owned())
}

fn media_type(headers: &HeaderMap, bytes: &[u8]) -> AppResult<&'static str> {
    let declared = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let actual = if bytes.starts_with(b"%PDF-1.") || bytes.starts_with(b"%PDF-2.0") {
        if bytes.len() < 14
            || !bytes[bytes.len().saturating_sub(1024)..]
                .windows(5)
                .any(|w| w == b"%%EOF")
        {
            None
        } else {
            Some("application/pdf")
        }
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes.get(8..16) == Some(b"\0\0\0\rIHDR")
        && bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82")
    {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") && bytes.ends_with(b"\xff\xd9") && bytes.len() >= 6
    {
        Some("image/jpeg")
    } else {
        None
    };
    match actual {
        Some(kind) if kind == declared => Ok(kind),
        _ => Err(AppError::bad_request(
            "Hanya PDF, PNG, atau JPEG dengan isi dan Content-Type yang sesuai yang diterima.",
        )),
    }
}

async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    bytes: Bytes,
) -> AppResult<Json<Value>> {
    let (_, role) = target(&kind)?;
    user.require(&[role])?;
    let filename = filename(&query.filename)?;
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(
            "Ukuran lampiran harus 1 byte sampai 5 MiB.",
        ));
    }
    let media_type = media_type(&headers, &bytes)?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let request = payload(
        "attachment.upload",
        &user,
        &json!({"document_type":kind,"document_id":id,"filename":filename,"media_type":media_type,"byte_size":bytes.len(),"sha256":digest}),
    );
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, query.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    lock_document(&mut tx, &kind, id).await?;
    target_exists(&mut tx, &kind, id).await?;
    check_quota(&mut tx, &kind, id, bytes.len() as i32).await?;
    let attachment_id = Uuid::new_v4();
    sqlx::query("INSERT INTO document_attachments(id,document_type,document_id,filename,media_type,content,sha256,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(attachment_id).bind(&kind).bind(id).bind(&filename).bind(media_type).bind(bytes.as_ref()).bind(&digest).bind(user.id).execute(&mut *tx).await?;
    event(&mut tx, &user, attachment_id, "uploaded", None, 1).await?;
    let result = metadata(&mut tx, attachment_id).await?;
    audit(&mut tx,&user,"attachment.uploaded","attachment",attachment_id,json!({"document_type":kind,"document_id":id,"filename":filename,"media_type":media_type,"byte_size":bytes.len(),"sha256":digest})).await?;
    remember(&mut tx, query.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

fn disposition(filename: &str) -> String {
    let fallback: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = filename
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'-' | b'_' | b'~') {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

async fn download(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let row = sqlx::query(
        "SELECT filename,media_type,content,sha256,byte_size FROM document_attachments WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(AppError::not_found)?;
    let filename: String = row.get("filename");
    let media_type: String = row.get("media_type");
    let bytes: Vec<u8> = row.get("content");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&media_type).map_err(|e| AppError::internal(e.into()))?,
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition(&filename)).map_err(|e| AppError::internal(e.into()))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string())
            .map_err(|e| AppError::internal(e.into()))?,
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok((headers, bytes).into_response())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchiveInput {
    version: i64,
    reason: String,
    idempotency_key: Uuid,
}

async fn event(
    tx: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    id: Uuid,
    action: &str,
    reason: Option<&str>,
    version: i64,
) -> AppResult<()> {
    sqlx::query("INSERT INTO document_attachment_events(id,attachment_id,action,reason,version,created_by) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(id).bind(action).bind(reason).bind(version).bind(user.id).execute(&mut **tx).await?;
    Ok(())
}

async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ArchiveInput>,
) -> AppResult<Json<Value>> {
    transition(state, user, id, body, true).await
}
async fn restore(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ArchiveInput>,
) -> AppResult<Json<Value>> {
    transition(state, user, id, body, false).await
}
async fn transition(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    body: ArchiveInput,
    archive: bool,
) -> AppResult<Json<Value>> {
    let reason = clean_text(&body.reason, 3, 1000, false)?;
    let action = if archive { "archived" } else { "restored" };
    let request = payload(
        if archive {
            "attachment.archive"
        } else {
            "attachment.restore"
        },
        &user,
        &json!({"id":id,"body":body}),
    );
    let mut tx = state.db.begin().await?;
    // Authorization precedes returning an idempotent result, including after a role change.
    let identity =
        sqlx::query("SELECT document_type,document_id FROM document_attachments WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(AppError::not_found)?;
    let kind: String = identity.get("document_type");
    let document_id: Uuid = identity.get("document_id");
    let (_, role) = target(&kind)?;
    user.require(&[role])?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    lock_document(&mut tx, &kind, document_id).await?;
    let row=sqlx::query("SELECT version,archived_at IS NOT NULL AS archived,byte_size FROM document_attachments WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_one(&mut *tx).await?;
    if body.version != row.get::<i64, _>("version") {
        return Err(AppError::conflict(
            "Lampiran berubah. Muat ulang sebelum melanjutkan.",
        ));
    }
    if row.get::<bool, _>("archived") == archive {
        return Err(AppError::conflict(if archive {
            "Lampiran sudah diarsipkan."
        } else {
            "Lampiran masih aktif."
        }));
    }
    if !archive {
        check_quota(&mut tx, &kind, document_id, row.get("byte_size")).await?;
    }
    if archive {
        sqlx::query("UPDATE document_attachments SET archived_at=now(),archived_by=$2,archive_reason=$3,version=version+1 WHERE id=$1")
            .bind(id).bind(user.id).bind(&reason).execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE document_attachments SET archived_at=NULL,archived_by=NULL,archive_reason=NULL,version=version+1 WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
    }
    event(&mut tx, &user, id, action, Some(&reason), body.version + 1).await?;
    let result = metadata(&mut tx, id).await?;
    audit(&mut tx,&user,&format!("attachment.{action}"),"attachment",id,json!({"document_type":kind,"document_id":document_id,"reason":reason,"version":body.version+1})).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
