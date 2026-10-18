use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn browser_entry_documents_do_not_cache_the_previous_application_bundle() {
    let directory = std::env::temp_dir().join(format!("haruflow-web-{}", Uuid::new_v4()));
    tokio::fs::create_dir(&directory).await.unwrap();
    let entry = directory.join("index.html");
    tokio::fs::write(
        &entry,
        "<!doctype html><main>Application version one</main>",
    )
    .await
    .unwrap();
    let app = crate::app_router(
        crate::AppState {
            db: PgPoolOptions::new()
                .connect_lazy("postgres://postgres@127.0.0.1/unused")
                .unwrap(),
            setup_token: "web-test-token-at-least-32-characters".into(),
            secure_cookies: false,
        },
        directory.clone(),
    );
    for path in ["/", "/index.html"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "Entry document {path}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("Application version one"));
    }
    tokio::fs::write(
        &entry,
        "<!doctype html><main>Application version two</main>",
    )
    .await
    .unwrap();
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("Application version two"));
    tokio::fs::remove_file(&entry).await.unwrap();
    tokio::fs::remove_dir(&directory).await.unwrap();
}
