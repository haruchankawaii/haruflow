mod auth;
#[cfg(test)]
mod auth_integration_tests;
mod business;
mod cli;
mod company;
#[cfg(test)]
mod company_tests;
mod corrections;
#[cfg(test)]
mod document_correction_tests;
mod document_corrections;
mod domain;
mod error;
mod insights;
#[cfg(test)]
mod insights_tests;
mod master_data;
mod opening;
#[cfg(test)]
mod opening_tests;
#[cfg(test)]
mod purchase_receipt_tests;
mod purchase_receipts;
mod purchasing;
#[cfg(test)]
mod purchasing_tests;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod upgrade_tests;
#[cfg(test)]
mod web_tests;

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub setup_token: String,
    pub secure_cookies: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "haruflow=info,tower_http=info".into()),
        )
        .init();
    let command = cli::parse(&env::args().skip(1).collect::<Vec<_>>())?;
    if command == cli::Command::Help {
        println!("{}", cli::HELP);
        return Ok(());
    }
    if command == cli::Command::Healthcheck {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let port = env::var("HARUFLOW_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse::<SocketAddr>()
            .context("Invalid HARUFLOW_BIND")?
            .port();
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
            socket
                .write_all(
                    b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .await?;
            let mut response = [0u8; 128];
            let size = socket.read(&mut response).await?;
            if !String::from_utf8_lossy(&response[..size]).starts_with("HTTP/1.1 200") {
                bail!("Application is not ready");
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Healthcheck timed out")??;
        return Ok(());
    }
    let database_url =
        env::var("DATABASE_URL").context("DATABASE_URL is required; see .env.example")?;
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await
        .context("Cannot connect to PostgreSQL")?;
    if command == cli::Command::Migrate {
        sqlx::migrate!("../../migrations")
            .run(&db)
            .await
            .context("Database migration failed")?;
        tracing::info!("database migrations complete");
        return Ok(());
    }
    if let cli::Command::ResetAdmin {
        email,
        password_stdin,
    } = command
    {
        let password = cli::read_recovery_password(password_stdin)?;
        company::reset_admin_password(&db, &email, password)
            .await
            .map_err(|error| anyhow::anyhow!("{}", error.message))?;
        println!("Password administrator diperbarui dan seluruh sesi lamanya dicabut.");
        return Ok(());
    }
    let setup_token = env::var("HARUFLOW_SETUP_TOKEN")
        .context("HARUFLOW_SETUP_TOKEN is required (at least 32 characters)")?;
    if setup_token.len() < 32 {
        bail!("HARUFLOW_SETUP_TOKEN must contain at least 32 characters");
    }
    let secure_cookies = match env::var("HARUFLOW_SECURE_COOKIES")
        .as_deref()
        .unwrap_or("true")
    {
        "true" => true,
        "false" => false,
        _ => bail!("HARUFLOW_SECURE_COOKIES must be true or false"),
    };
    let state = AppState {
        db,
        setup_token,
        secure_cookies,
    };
    let bind: SocketAddr = env::var("HARUFLOW_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .context("Invalid HARUFLOW_BIND address")?;
    let static_dir =
        PathBuf::from(env::var("HARUFLOW_STATIC_DIR").unwrap_or_else(|_| "apps/web/dist".into()));
    let app = app_router(state, static_dir);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address = %listener.local_addr()?, "Haruflow Community is ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn app_router(state: AppState, static_dir: PathBuf) -> Router {
    let api = auth::router()
        .merge(company::router())
        .merge(master_data::router())
        .merge(business::router())
        .merge(corrections::router())
        .merge(opening::router())
        .merge(insights::router())
        .merge(document_corrections::router())
        .merge(purchasing::router())
        .merge(purchase_receipts::router())
        .fallback(|| async { error::AppError::not_found() })
        .layer(middleware::from_fn(api_guard));
    Router::new()
        .route(
            "/health/live",
            get(|| async { Json(json!({"status":"ok"})) }),
        )
        .route("/health/ready", get(ready))
        .nest("/api", api)
        .fallback_service(
            ServeDir::new(&static_dir)
                .not_found_service(ServeFile::new(static_dir.join("index.html"))),
        )
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"status":"ready"}))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"unavailable"})),
        ),
    }
}

async fn api_guard(request: Request, next: Next) -> Response {
    // Cross-origin forms cannot set this header and no cross-origin CORS access is granted.
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && request
        .headers()
        .get("x-haruflow-request")
        .and_then(|v| v.to_str().ok())
        != Some("1")
    {
        return error::AppError::forbidden().into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    // The entry document selects fingerprinted assets. Caching an older HTML
    // shell can hide newly deployed screens even when the server is upgraded.
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"))
    {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"));
    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
