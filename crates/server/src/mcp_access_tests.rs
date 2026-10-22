use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppResult,
    mcp_access::{self, McpPrincipal},
    supplier_bill_tests::{actor, call, fixture, request},
};

fn probe_app(pool: &PgPool) -> Router {
    async fn read(
        State(state): State<AppState>,
        principal: McpPrincipal,
        Path(scope): Path<String>,
    ) -> AppResult<Json<Value>> {
        let authorized = principal.require_scope(&scope);
        mcp_access::audit_call(&state, &principal, "test.read", authorized.is_ok()).await?;
        authorized?;
        Ok(Json(
            json!({"user_id":principal.user.id,"scopes":principal.scopes}),
        ))
    }
    Router::new()
        .route("/probe/{scope}", get(read))
        .with_state(AppState {
            db: pool.clone(),
            setup_token: "mcp-test-only".into(),
            secure_cookies: false,
        })
}

async fn probe(app: &Router, token: &str, scope: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/probe/{scope}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn grant(app: &Router, admin: &str, user: Uuid, scopes: &[&str]) {
    call(
        app,
        admin,
        "PUT",
        &format!("/api/ai/access/{user}"),
        Some(json!({"enabled":true,"scopes":scopes})),
    )
    .await;
}

async fn token(app: &Router, cookie: &str, scopes: &[&str]) -> Value {
    call(
        app,
        cookie,
        "POST",
        "/api/ai/tokens",
        Some(json!({"name":"Codex Windows","scopes":scopes,"expires_in_days":30})),
    )
    .await
}

async fn login_cookie(app: &Router, user: Uuid, password: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("x-haruflow-request", "1")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"email":format!("{user}@example.test"),"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[sqlx::test(migrations = "../../migrations")]
async fn password_reset_change_and_user_status_changes_permanently_revoke_ai_credentials(
    pool: PgPool,
) {
    let (app, _, admin_cookie) = fixture(&pool).await;
    let (finance, finance_cookie) = actor(&pool, "finance").await;
    let reads = probe_app(&pool);
    grant(&app, &admin_cookie, finance, &["finance", "sales"]).await;
    let issued = token(&app, &finance_cookie, &["finance"]).await;
    let password = "mcp-reset-password-long-enough";
    call(
        &app,
        &admin_cookie,
        "POST",
        &format!("/api/users/{finance}/password"),
        Some(json!({"password":password})),
    )
    .await;
    assert_eq!(
        probe(&reads, issued["token"].as_str().unwrap(), "finance")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let cookie = login_cookie(&app, finance, password).await;
    let issued = token(&app, &cookie, &["finance"]).await;
    let changed = "mcp-changed-password-long-enough";
    call(
        &app,
        &cookie,
        "POST",
        "/api/auth/password",
        Some(json!({"current_password":password,"password":changed})),
    )
    .await;
    assert_eq!(
        probe(&reads, issued["token"].as_str().unwrap(), "finance")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let cookie = login_cookie(&app, finance, changed).await;
    let issued = token(&app, &cookie, &["finance"]).await;
    call(
        &app,
        &admin_cookie,
        "PATCH",
        &format!("/api/users/{finance}"),
        Some(json!({"active":false})),
    )
    .await;
    call(
        &app,
        &admin_cookie,
        "PATCH",
        &format!("/api/users/{finance}"),
        Some(json!({"active":true})),
    )
    .await;
    assert_eq!(
        probe(&reads, issued["token"].as_str().unwrap(), "finance")
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "reactivation must not revive an old AI token"
    );
    let cookie = login_cookie(&app, finance, changed).await;
    let issued = token(&app, &cookie, &["sales"]).await;
    call(
        &app,
        &admin_cookie,
        "PATCH",
        &format!("/api/users/{finance}"),
        Some(json!({"role":"sales"})),
    )
    .await;
    assert_eq!(
        probe(&reads, issued["token"].as_str().unwrap(), "sales")
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "role changes rotate credentials even if an old scope remains allowed"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_admin_password_recovery_revokes_ai_tokens_in_the_same_transaction(pool: PgPool) {
    let (app, admin, cookie) = fixture(&pool).await;
    grant(&app, &cookie, admin, &["finance"]).await;
    let issued = token(&app, &cookie, &["finance"]).await;
    crate::company::reset_admin_password(
        &pool,
        &format!("{admin}@example.test"),
        "mcp-local-recovery-password-long-enough".into(),
    )
    .await
    .unwrap();
    assert_eq!(
        probe(
            &probe_app(&pool),
            issued["token"].as_str().unwrap(),
            "finance"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let revoked: bool =
        sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM ai_tokens WHERE user_id=$1")
            .bind(admin)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(revoked);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ai_connections_are_opt_in_personal_role_scoped_and_secret_only_once(pool: PgPool) {
    let (app, admin, admin_cookie) = fixture(&pool).await;
    let (sales, sales_cookie) = actor(&pool, "sales").await;
    let access = call(&app, &admin_cookie, "GET", "/api/ai/access", None).await;
    assert_eq!(access["access"]["enabled"], false);
    assert_eq!(access["access"]["scopes"], json!([]));
    assert_eq!(access["users"].as_array().unwrap().len(), 2);
    let own = call(&app, &sales_cookie, "GET", "/api/ai/access", None).await;
    assert!(own.get("users").is_none());
    assert_eq!(own["access"]["allowed_scopes"], json!(["sales"]));
    assert_eq!(
        request(
            &app,
            &admin_cookie,
            "POST",
            "/api/ai/tokens",
            Some(json!({"name":"No consent","scopes":["finance"]}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            &sales_cookie,
            "PUT",
            &format!("/api/ai/access/{sales}"),
            Some(json!({"enabled":true,"scopes":["sales"]}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            &admin_cookie,
            "PUT",
            &format!("/api/ai/access/{sales}"),
            Some(json!({"enabled":true,"scopes":["finance"]}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    grant(&app, &admin_cookie, sales, &["sales"]).await;
    let issued = token(&app, &sales_cookie, &["sales"]).await;
    let secret = issued["token"].as_str().unwrap();
    assert_eq!(secret.len(), 71);
    assert!(secret.starts_with("hf_mcp_"));
    let hash: String = sqlx::query_scalar("SELECT token_hash FROM ai_tokens WHERE user_id=$1")
        .bind(sales)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hash, hex::encode(Sha256::digest(secret.as_bytes())));
    let listing = call(&app, &sales_cookie, "GET", "/api/ai/access", None).await;
    assert!(!listing.to_string().contains(secret));
    assert!(!listing.to_string().contains(&hash));
    assert_eq!(listing["tokens"][0]["id"], issued["connection"]["id"]);
    assert_eq!(listing["access"]["user_id"], sales.to_string());
    assert!(
        call(&app, &admin_cookie, "GET", "/api/ai/access", None).await["tokens"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let probe_app = probe_app(&pool);
    assert_eq!(probe(&probe_app, secret, "sales").await.0, StatusCode::OK);
    assert_eq!(
        probe(&probe_app, secret, "finance").await.0,
        StatusCode::FORBIDDEN
    );
    let audits:String=sqlx::query_scalar("SELECT COALESCE(string_agg(details::text,''),'') FROM audit_events WHERE action LIKE 'ai.%'").fetch_one(&pool).await.unwrap();
    assert!(!audits.contains(secret));
    assert!(!audits.contains(&hash));
    assert!(audits.contains("test.read"));
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        users, 2,
        "issuing MCP tokens must not create an eleventh-style service account"
    );
    assert_ne!(admin, sales);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ai_reads_recheck_role_grant_expiry_revocation_and_active_user_each_time(pool: PgPool) {
    let (app, admin, cookie) = fixture(&pool).await;
    grant(&app, &cookie, admin, &["finance", "sales", "inventory"]).await;
    let issued = token(&app, &cookie, &["finance", "sales", "inventory"]).await;
    let secret = issued["token"].as_str().unwrap();
    let id: Uuid = issued["connection"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let reads = probe_app(&pool);
    assert_eq!(probe(&reads, secret, "finance").await.0, StatusCode::OK);
    grant(&app, &cookie, admin, &["sales"]).await;
    assert_eq!(
        probe(&reads, secret, "finance").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(probe(&reads, secret, "sales").await.0, StatusCode::OK);
    sqlx::query("UPDATE users SET role='warehouse' WHERE id=$1")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        probe(&reads, secret, "sales").await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE users SET role='admin',active=false WHERE id=$1")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        probe(&reads, secret, "sales").await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE users SET active=true WHERE id=$1")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE ai_tokens SET created_at=now()-interval '2 days',expires_at=now()-interval '1 day' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        probe(&reads, secret, "sales").await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query(
        "UPDATE ai_tokens SET expires_at=now()+interval '1 day',revoked_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        probe(&reads, secret, "sales").await.0,
        StatusCode::UNAUTHORIZED
    );
    let used: bool =
        sqlx::query_scalar("SELECT last_used_at IS NOT NULL FROM ai_tokens WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(used);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ai_grant_disable_permanently_revokes_old_tokens_and_only_owner_or_admin_can_revoke(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let (finance, finance_cookie) = actor(&pool, "finance").await;
    let (_, sales_cookie) = actor(&pool, "sales").await;
    grant(&app, &cookie, finance, &["finance"]).await;
    let issued = token(&app, &finance_cookie, &["finance"]).await;
    let path = format!(
        "/api/ai/tokens/{}",
        issued["connection"]["id"].as_str().unwrap()
    );
    assert_eq!(
        request(&app, &sales_cookie, "DELETE", &path, None).await.0,
        StatusCode::FORBIDDEN
    );
    call(&app, &cookie, "DELETE", &path, None).await;
    assert_eq!(
        probe(
            &probe_app(&pool),
            issued["token"].as_str().unwrap(),
            "finance"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let issued = token(&app, &finance_cookie, &["finance"]).await;
    call(
        &app,
        &cookie,
        "PUT",
        &format!("/api/ai/access/{finance}"),
        Some(json!({"enabled":false,"scopes":[]})),
    )
    .await;
    assert_eq!(
        probe(
            &probe_app(&pool),
            issued["token"].as_str().unwrap(),
            "finance"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    grant(&app, &cookie, finance, &["finance"]).await;
    assert_eq!(
        probe(
            &probe_app(&pool),
            issued["token"].as_str().unwrap(),
            "finance"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let issued = token(&app, &finance_cookie, &["finance"]).await;
    let path = format!(
        "/api/ai/tokens/{}",
        issued["connection"]["id"].as_str().unwrap()
    );
    call(&app, &finance_cookie, "DELETE", &path, None).await;
    assert_eq!(
        probe(
            &probe_app(&pool),
            issued["token"].as_str().unwrap(),
            "finance"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_access_grants WHERE user_id=$1")
        .bind(admin)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 0, "admin never implicitly receives an AI grant");
}

#[sqlx::test(migrations = "../../migrations")]
async fn ai_credentials_do_not_authenticate_browser_endpoints_and_cookie_does_not_authenticate_ai(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    grant(&app, &cookie, admin, &["finance"]).await;
    let issued = token(&app, &cookie, &["finance"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/ai/access")
                .header(
                    "authorization",
                    format!("Bearer {}", issued["token"].as_str().unwrap()),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = probe_app(&pool)
        .oneshot(
            Request::builder()
                .uri("/probe/finance")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    for scopes in [json!([]), json!(["all"]), json!(["inventory"])] {
        let result = request(
            &app,
            &cookie,
            "POST",
            "/api/ai/tokens",
            Some(json!({"name":"Invalid scopes","scopes":scopes,"expires_in_days":30})),
        )
        .await;
        assert!(!result.0.is_success());
    }
    for days in [0, 91] {
        assert_eq!(
            request(
                &app,
                &cookie,
                "POST",
                "/api/ai/tokens",
                Some(json!({"name":"Invalid expiry","scopes":["finance"],"expires_in_days":days}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for _ in 0..3 {
        token(&app, &cookie, &["finance"]).await;
    }
    let a = request(
        &app,
        &cookie,
        "POST",
        "/api/ai/tokens",
        Some(json!({"name":"Concurrent A","scopes":["finance"]})),
    );
    let b = request(
        &app,
        &cookie,
        "POST",
        "/api/ai/tokens",
        Some(json!({"name":"Concurrent B","scopes":["finance"]})),
    );
    let (a, b) = tokio::join!(a, b);
    assert_eq!(
        usize::from(a.0.is_success()) + usize::from(b.0.is_success()),
        1
    );
    assert!(a.0 == StatusCode::CONFLICT || b.0 == StatusCode::CONFLICT);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM ai_tokens WHERE user_id=$1 AND revoked_at IS NULL AND expires_at>now()")
        .bind(admin).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 5);
}
