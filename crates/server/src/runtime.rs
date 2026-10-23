use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};
use std::time::Duration;

use crate::AppState;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub fn version() -> Value {
    json!({"application":"Haruflow Community","version":env!("CARGO_PKG_VERSION"),
        "commit":env!("HARUFLOW_BUILD_COMMIT"),
        "schema_version":MIGRATIONS.iter().last().map(|migration|migration.version).unwrap_or(0)})
}

async fn compatible(db: &PgPool) -> bool {
    let Ok(mut connection) = db.acquire().await else {
        return false;
    };
    compatible_connection(&mut connection).await
}

pub(crate) async fn compatible_connection(db: &mut PgConnection) -> bool {
    let Ok(rows) =
        sqlx::query("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(db)
            .await
    else {
        return false;
    };
    rows.len() == MIGRATIONS.iter().count()
        && rows.iter().zip(MIGRATIONS.iter()).all(|(row, migration)| {
            row.get::<i64, _>("version") == migration.version
                && row.get::<bool, _>("success")
                && row.get::<Vec<u8>, _>("checksum").as_slice() == migration.checksum.as_ref()
        })
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    if tokio::time::timeout(Duration::from_secs(2), compatible(&state.db))
        .await
        .unwrap_or(false)
    {
        (StatusCode::OK, Json(json!({"status":"ready"})))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"unavailable"})),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = false)]
    async fn readiness_requires_every_matching_successful_migration(pool: PgPool) {
        assert!(!compatible(&pool).await);
        MIGRATIONS.run(&pool).await.unwrap();
        assert!(compatible(&pool).await);
        let last = MIGRATIONS.iter().last().unwrap().version;
        sqlx::query("UPDATE _sqlx_migrations SET success=false WHERE version=$1")
            .bind(last)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!compatible(&pool).await);
        sqlx::query(
            "UPDATE _sqlx_migrations SET success=true,checksum='wrong'::bytea WHERE version=$1",
        )
        .bind(last)
        .execute(&pool)
        .await
        .unwrap();
        assert!(!compatible(&pool).await);
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=$1")
            .bind(last)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!compatible(&pool).await);
    }

    #[test]
    fn public_version_contains_only_build_identity() {
        let value = version();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert_eq!(value["application"], "Haruflow Community");
        assert_eq!(
            value["schema_version"],
            MIGRATIONS.iter().last().unwrap().version
        );
        let commit = value["commit"].as_str().unwrap();
        assert!(
            commit == "unknown"
                || (commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit()))
        );
    }
}
