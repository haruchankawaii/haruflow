use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    company,
    supplier_bill_tests::{actor, call, fixture},
};

async fn issue(app: &Router, admin: &str, user: Uuid, cookie: &str) -> String {
    call(
        app,
        admin,
        "PUT",
        &format!("/api/ai/access/{user}"),
        Some(json!({"enabled":true,"scopes":["finance"]})),
    )
    .await;
    call(
        app,
        cookie,
        "POST",
        "/api/ai/tokens",
        Some(json!({"name":"Recovery fixture","scopes":["finance"]})),
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn profile_status(app: &Router, token: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .uri("/api/ai/mcp/profile")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_restore_recovery_revokes_every_user_and_preserves_previous_revocations(
    pool: PgPool,
) {
    let (app, admin, admin_cookie) = fixture(&pool).await;
    let (finance, finance_cookie) = actor(&pool, "finance").await;
    let admin_token = issue(&app, &admin_cookie, admin, &admin_cookie).await;
    let finance_token = issue(&app, &admin_cookie, finance, &finance_cookie).await;
    assert_eq!(profile_status(&app, &admin_token).await, StatusCode::OK);
    assert_eq!(profile_status(&app, &finance_token).await, StatusCode::OK);
    // Keep a previously revoked credential's original timestamp intact.
    sqlx::query("INSERT INTO ai_tokens(id,user_id,name,token_hash,scopes,expires_at,revoked_at) VALUES($1,$2,'Already revoked',$3,ARRAY['finance'],now()+interval '1 day','2020-01-01Z')")
        .bind(Uuid::new_v4()).bind(finance).bind("a".repeat(64)).execute(&pool).await.unwrap();
    assert_eq!(
        company::revoke_ai_tokens_after_restore(&pool, &format!("{admin}@EXAMPLE.TEST"))
            .await
            .unwrap(),
        2
    );
    for token in [&admin_token, &finance_token] {
        assert_eq!(profile_status(&app, token).await, StatusCode::UNAUTHORIZED);
    }
    let previous: bool = sqlx::query_scalar(
        "SELECT revoked_at='2020-01-01Z' FROM ai_tokens WHERE name='Already revoked'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(previous);
    let (who, details): (Uuid, Value) = sqlx::query_as(
        "SELECT actor_id,details FROM audit_events WHERE action='ai.tokens.revoked_local'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(who, admin);
    assert_eq!(
        details,
        json!({"revoked_count":2,"source":"local_operator_recovery"})
    );
    let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_access_grants WHERE enabled")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 2);
    let ledger: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ledger, 0);
    assert_eq!(
        company::revoke_ai_tokens_after_restore(&pool, &format!("{admin}@example.test"))
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_restore_recovery_rejects_unknown_inactive_and_non_admin_without_changes(
    pool: PgPool,
) {
    let (app, admin, cookie) = fixture(&pool).await;
    let token = issue(&app, &cookie, admin, &cookie).await;
    let (finance, _) = actor(&pool, "finance").await;
    let (inactive, _) = actor(&pool, "admin").await;
    sqlx::query("UPDATE users SET active=false WHERE id=$1")
        .bind(inactive)
        .execute(&pool)
        .await
        .unwrap();
    for email in [
        format!("{finance}@example.test"),
        format!("{inactive}@example.test"),
        "missing@example.test".into(),
        "bad-email".into(),
    ] {
        assert!(
            company::revoke_ai_tokens_after_restore(&pool, &email)
                .await
                .is_err()
        );
        assert_eq!(profile_status(&app, &token).await, StatusCode::OK);
    }
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action='ai.tokens.revoked_local'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 0);
}
