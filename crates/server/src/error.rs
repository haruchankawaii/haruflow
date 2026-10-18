use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "validation",
            message: message.into(),
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "Silakan masuk untuk melanjutkan.".into(),
        }
    }

    pub fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: "Anda tidak memiliki izin untuk tindakan ini.".into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            message: message.into(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "Data tidak ditemukan.".into(),
        }
    }

    pub fn internal(error: anyhow::Error) -> Self {
        tracing::error!(error = %error, "request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "Terjadi kesalahan pada server. Silakan coba lagi.".into(),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::RowNotFound => Self::not_found(),
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                Self::conflict("Data dengan identitas yang sama sudah ada.")
            }
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                Self::bad_request("Referensi data tidak valid atau masih digunakan.")
            }
            sqlx::Error::Database(db) if db.is_check_violation() => {
                Self::bad_request("Data tidak memenuhi aturan validasi.")
            }
            _ => Self::internal(error.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "message": self.message } })),
        )
            .into_response()
    }
}
