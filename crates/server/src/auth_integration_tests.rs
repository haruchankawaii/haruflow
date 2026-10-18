use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{AppState, app_router};

const SETUP_TOKEN: &str = "identity-integration-setup-token-over-32-characters";
const INITIAL_PASSWORD: &str = "identity-initial-password-strong";
const RESET_PASSWORD: &str = "identity-reset-password-strong";
const CHANGED_PASSWORD: &str = "identity-changed-password-strong";

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, Option<String>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-haruflow-request", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response.headers().get("set-cookie").map(|value| {
        let value = value.to_str().unwrap();
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("SameSite=Strict"));
        value.split(';').next().unwrap().to_owned()
    });
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
    (status, body, cookie)
}

fn success(status: StatusCode, body: &Value) {
    assert!(status.is_success(), "{status}: {body}");
}

async fn login(app: &Router, email: &str, password: &str) -> (Value, String) {
    let (status, body, cookie) = request(
        app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email": email, "password": password})),
    )
    .await;
    success(status, &body);
    (body, cookie.expect("login must issue a session cookie"))
}

async fn session_status(app: &Router, cookie: &str, expected: StatusCode) {
    let (status, body, _) = request(app, "GET", "/api/auth/me", Some(cookie), None).await;
    assert_eq!(status, expected, "{body}");
}

async fn session_count(pool: &PgPool, user_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn identity_setup_race_roles_and_session_revocation(pool: PgPool) {
    let app = app_router(
        AppState {
            db: pool.clone(),
            setup_token: SETUP_TOKEN.into(),
            secure_cookies: false,
        },
        ".local/identity-test-no-static-files".into(),
    );
    let setup_a = json!({
        "token": SETUP_TOKEN, "company_name": "Concurrent Company A", "name": "Owner A",
        "email": "identity-owner-a@example.test", "password": INITIAL_PASSWORD
    });
    let setup_b = json!({
        "token": SETUP_TOKEN, "company_name": "Concurrent Company B", "name": "Owner B",
        "email": "identity-owner-b@example.test", "password": INITIAL_PASSWORD
    });
    // Both requests must reach the transactional singleton check: exactly one may commit.
    let (a, b) = tokio::join!(
        request(&app, "POST", "/api/setup", None, Some(setup_a)),
        request(&app, "POST", "/api/setup", None, Some(setup_b)),
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|status| status.is_success())
            .count(),
        1,
        "setup outcomes: {} {}, bodies: {} {}",
        a.0,
        b.0,
        a.1,
        b.1
    );
    assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
    let winner = if a.0.is_success() { a } else { b };
    let admin_id = Uuid::parse_str(winner.1["user"]["id"].as_str().unwrap()).unwrap();
    let admin_cookie = winner.2.unwrap();
    let companies: i64 = sqlx::query_scalar("SELECT count(*) FROM company")
        .fetch_one(&pool)
        .await
        .unwrap();
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (companies, users, session_count(&pool, admin_id).await),
        (1, 1, 1)
    );
    let (_, status, _) = request(&app, "GET", "/api/setup/status", None, None).await;
    assert_eq!(status["needs_setup"], false);

    for body in [json!({"active": false}), json!({"role": "viewer"})] {
        let (status, _, _) = request(
            &app,
            "PATCH",
            &format!("/api/users/{admin_id}"),
            Some(&admin_cookie),
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    session_status(&app, &admin_cookie, StatusCode::OK).await;

    let email = format!("identity-sales-{}@example.test", Uuid::new_v4());
    let (status, created, _) = request(
        &app, "POST", "/api/users", Some(&admin_cookie),
        Some(json!({"name": "Sales User", "email": email, "password": INITIAL_PASSWORD, "role": "sales"})),
    ).await;
    success(status, &created);
    let user_id = Uuid::parse_str(created["user"]["id"].as_str().unwrap()).unwrap();
    let user_path = format!("/api/users/{user_id}");
    let (_, listed, _) = request(&app, "GET", "/api/users", Some(&admin_cookie), None).await;
    assert!(!listed.to_string().contains("password_hash"));
    assert!(!listed.to_string().contains("token_hash"));
    let (context, cookie_a) = login(&app, &email, INITIAL_PASSWORD).await;
    assert_eq!(context["user"]["role"], "sales");
    let (_, cookie_b) = login(&app, &email, INITIAL_PASSWORD).await;
    assert_eq!(session_count(&pool, user_id).await, 2);

    let (status, body, _) = request(
        &app,
        "PATCH",
        &user_path,
        Some(&admin_cookie),
        Some(json!({"role": "viewer"})),
    )
    .await;
    success(status, &body);
    assert_eq!(body["user"]["role"], "viewer");
    assert_eq!(session_count(&pool, user_id).await, 0);
    session_status(&app, &cookie_a, StatusCode::UNAUTHORIZED).await;
    session_status(&app, &cookie_b, StatusCode::UNAUTHORIZED).await;
    let (context, viewer_cookie) = login(&app, &email, INITIAL_PASSWORD).await;
    assert_eq!(context["user"]["role"], "viewer");
    let (status, _, _) = request(
        &app,
        "PATCH",
        &user_path,
        Some(&viewer_cookie),
        Some(json!({"role": "admin"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = request(&app, "GET", "/api/users", Some(&viewer_cookie), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body, _) = request(
        &app,
        "PATCH",
        &user_path,
        Some(&admin_cookie),
        Some(json!({"active": false})),
    )
    .await;
    success(status, &body);
    assert_eq!(session_count(&pool, user_id).await, 0);
    session_status(&app, &viewer_cookie, StatusCode::UNAUTHORIZED).await;
    let (status, _, cookie) = request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email": email, "password": INITIAL_PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none());

    let (status, body, _) = request(
        &app,
        "PATCH",
        &user_path,
        Some(&admin_cookie),
        Some(json!({"active": true})),
    )
    .await;
    success(status, &body);
    let (_, before_reset_a) = login(&app, &email, INITIAL_PASSWORD).await;
    let (_, before_reset_b) = login(&app, &email, INITIAL_PASSWORD).await;
    let (status, body, _) = request(
        &app,
        "POST",
        &format!("{user_path}/password"),
        Some(&admin_cookie),
        Some(json!({"password": RESET_PASSWORD})),
    )
    .await;
    success(status, &body);
    assert_eq!(session_count(&pool, user_id).await, 0);
    session_status(&app, &before_reset_a, StatusCode::UNAUTHORIZED).await;
    session_status(&app, &before_reset_b, StatusCode::UNAUTHORIZED).await;
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email": email, "password": INITIAL_PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (_, before_change_a) = login(&app, &email, RESET_PASSWORD).await;
    let (_, before_change_b) = login(&app, &email, RESET_PASSWORD).await;
    let (status, _, cookie) = request(
        &app,
        "POST",
        "/api/auth/password",
        Some(&before_change_a),
        Some(
            json!({"current_password": "incorrect-current-password", "password": CHANGED_PASSWORD}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none());
    session_status(&app, &before_change_a, StatusCode::OK).await;
    let (status, body, changed_cookie) = request(
        &app,
        "POST",
        "/api/auth/password",
        Some(&before_change_a),
        Some(json!({"current_password": RESET_PASSWORD, "password": CHANGED_PASSWORD})),
    )
    .await;
    success(status, &body);
    let changed_cookie = changed_cookie.unwrap();
    assert_ne!(changed_cookie, before_change_a);
    assert_eq!(session_count(&pool, user_id).await, 1);
    session_status(&app, &before_change_a, StatusCode::UNAUTHORIZED).await;
    session_status(&app, &before_change_b, StatusCode::UNAUTHORIZED).await;
    session_status(&app, &changed_cookie, StatusCode::OK).await;
    let (status, _, _) = request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"email": email, "password": RESET_PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, final_cookie) = login(&app, &email, CHANGED_PASSWORD).await;

    // A cross-origin form cannot revoke the session without the required custom header.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", &final_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    session_status(&app, &final_cookie, StatusCode::OK).await;

    let (status, audit, _) =
        request(&app, "GET", "/api/audit-events", Some(&admin_cookie), None).await;
    success(status, &audit);
    let actions: Vec<&str> = audit["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["action"].as_str())
        .collect();
    for action in [
        "company.setup",
        "user.create",
        "user.update",
        "user.password_reset",
        "user.password_change",
    ] {
        assert!(actions.contains(&action), "missing audit action {action}");
    }
    for secret in [
        INITIAL_PASSWORD,
        RESET_PASSWORD,
        CHANGED_PASSWORD,
        "password_hash",
        "token_hash",
    ] {
        assert!(
            !audit.to_string().contains(secret),
            "audit must not expose {secret}"
        );
    }
    let (status, _, _) = request(&app, "GET", "/api/audit-events", Some(&final_cookie), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        sqlx::query("DELETE FROM audit_events")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("TRUNCATE audit_events")
            .execute(&pool)
            .await
            .is_err()
    );
    session_status(&app, &admin_cookie, StatusCode::OK).await;
}
