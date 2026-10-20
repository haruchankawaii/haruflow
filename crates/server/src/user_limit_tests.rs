use crate::supplier_bill_tests::{actor, call, fixture, request};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::borrow::Cow;
use uuid::Uuid;

fn create_body() -> Value {
    json!({"name":"Pengguna Community","email":format!("{}@example.test",Uuid::new_v4()),"role":"viewer","password":"community-test-password-2026"})
}
async fn insert_user(pool: &PgPool, active: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,name,email,password_hash,role,active) VALUES($1,'Capacity tester',$2,'unused-fixture','viewer',$3)")
        .bind(id).bind(format!("{id}@example.test")).bind(active).execute(pool).await.unwrap();
    id
}
async fn active(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users WHERE active")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn fill(pool: &PgPool, target: i64) {
    while active(pool).await < target {
        insert_user(pool, true).await;
    }
}
async fn snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('users',(SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM users u),'sessions',(SELECT jsonb_agg(to_jsonb(s) ORDER BY token_hash) FROM sessions s),'audits',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM audit_events a))").fetch_one(pool).await.unwrap()
}
fn user_path(id: Uuid) -> String {
    format!("/api/users/{id}")
}

#[sqlx::test(migrations = "../../migrations")]
async fn community_user_limit_usage_permissions_slot_reuse_and_failed_write_rollback(pool: PgPool) {
    let (app, admin, cookie) = fixture(&pool).await;
    let (_, sales) = actor(&pool, "sales").await;
    let (inactive, _) = actor(&pool, "viewer").await;
    sqlx::query("UPDATE users SET active=false WHERE id=$1")
        .bind(inactive)
        .execute(&pool)
        .await
        .unwrap();
    fill(&pool, 10).await;
    let list = call(&app, &cookie, "GET", "/api/users", None).await;
    assert_eq!(list["usage"], json!({"active":10,"limit":10,"available":0}));
    assert_eq!(list["items"].as_array().unwrap().len(), 11);
    assert_eq!(
        request(&app, &sales, "GET", "/api/users", None).await.0,
        403
    );
    assert_eq!(
        request(&app, &sales, "POST", "/api/users", Some(create_body()))
            .await
            .0,
        403
    );
    let before = snapshot(&pool).await;
    let (status, error) = request(&app, &cookie, "POST", "/api/users", Some(create_body())).await;
    assert_eq!(status, 409);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("10 pengguna aktif")
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "PATCH",
            &user_path(inactive),
            Some(json!({"active":true}))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "PATCH",
            &user_path(admin),
            Some(json!({"active":false}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &cookie,
            "PATCH",
            &user_path(admin),
            Some(json!({"role":"viewer"}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        snapshot(&pool).await,
        before,
        "Failed capacity/self-admin requests must preserve users, sessions and audit rows"
    );
    // Updating an already active user or an inactive user's role consumes no slot.
    call(
        &app,
        &cookie,
        "PATCH",
        &user_path(inactive),
        Some(json!({"role":"finance"})),
    )
    .await;
    let removable: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE active AND role='viewer' ORDER BY id LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    call(
        &app,
        &cookie,
        "PATCH",
        &user_path(removable),
        Some(json!({"role":"warehouse","active":true})),
    )
    .await;
    call(
        &app,
        &cookie,
        "PATCH",
        &user_path(removable),
        Some(json!({"active":false})),
    )
    .await;
    let list = call(&app, &cookie, "GET", "/api/users", None).await;
    assert_eq!(list["usage"], json!({"active":9,"limit":10,"available":1}));
    let restored = call(
        &app,
        &cookie,
        "PATCH",
        &user_path(inactive),
        Some(json!({"active":true})),
    )
    .await;
    assert_eq!(restored["user"]["active"], true);
    assert_eq!(restored["user"]["role"], "finance");
    assert_eq!(active(&pool).await, 10);
    let stale_sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id=$1")
        .bind(inactive)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        stale_sessions, 0,
        "Successful reactivation must preserve existing session revocation behavior"
    );
    // Password resets remain usable at capacity and do not change active counts.
    call(
        &app,
        &cookie,
        "POST",
        &format!("/api/users/{inactive}/password"),
        Some(json!({"password":"new-community-password-2026"})),
    )
    .await;
    assert_eq!(active(&pool).await, 10);
}

#[sqlx::test(migrations = "../../migrations")]
async fn community_user_limit_concurrent_api_creates_share_last_slot(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    fill(&pool, 9).await;
    let before_audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action='user.create'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", "/api/users", Some(create_body())),
        request(&app, &cookie, "POST", "/api/users", Some(create_body()))
    );
    assert!(
        matches!((a.0.as_u16(), b.0.as_u16()), (200, 409) | (409, 200)),
        "{a:?} / {b:?}"
    );
    assert_eq!(active(&pool).await, 10);
    let audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action='user.create'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, before_audits + 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn community_user_limit_create_and_reactivate_cannot_both_take_last_slot(pool: PgPool) {
    let (app, _, cookie) = fixture(&pool).await;
    let inactive = insert_user(&pool, false).await;
    fill(&pool, 9).await;
    let path = user_path(inactive);
    let (a, b) = tokio::join!(
        request(&app, &cookie, "POST", "/api/users", Some(create_body())),
        request(&app, &cookie, "PATCH", &path, Some(json!({"active":true})))
    );
    assert!(
        matches!((a.0.as_u16(), b.0.as_u16()), (200, 409) | (409, 200)),
        "{a:?} / {b:?}"
    );
    assert_eq!(active(&pool).await, 10);
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action IN ('user.create','user.update')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn community_user_limit_database_guard_serializes_direct_activation_and_bulk_writes(
    pool: PgPool,
) {
    let (_, _, _) = fixture(&pool).await;
    let inactive = insert_user(&pool, false).await;
    fill(&pool, 9).await;
    let new_id = Uuid::new_v4();
    let insert=sqlx::query("INSERT INTO users(id,name,email,password_hash,role) VALUES($1,'Competing SQL user',$2,'unused','viewer')").bind(new_id).bind(format!("{new_id}@example.test"));
    let reactivate = sqlx::query("UPDATE users SET active=true WHERE id=$1").bind(inactive);
    let (a, b) = tokio::join!(insert.execute(&pool), reactivate.execute(&pool));
    assert_eq!(
        usize::from(a.is_ok()) + usize::from(b.is_ok()),
        1,
        "{a:?} / {b:?}"
    );
    let error = if let Err(error) = a {
        error
    } else {
        b.unwrap_err()
    };
    assert_eq!(
        error.as_database_error().unwrap().constraint(),
        Some("community_active_user_limit")
    );
    assert_eq!(active(&pool).await, 10);
    let archived = insert_user(&pool, false).await;
    sqlx::query(
        "UPDATE users SET role='finance',password_hash='changed-while-inactive' WHERE id=$1",
    )
    .bind(archived)
    .execute(&pool)
    .await
    .unwrap();
    let before = snapshot(&pool).await;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert!(sqlx::query("INSERT INTO users(id,name,email,password_hash,role,active) VALUES($1,'Inactive batch row',$2,'unused','viewer',false),($3,'Active batch row',$4,'unused','viewer',true)")
        .bind(a).bind(format!("{a}@example.test")).bind(b).bind(format!("{b}@example.test")).execute(&pool).await.is_err());
    assert_eq!(
        snapshot(&pool).await,
        before,
        "A rejected multi-row insert must roll back its inactive row as well"
    );
}

#[sqlx::test(migrations = false)]
async fn community_user_limit_upgrade_preserves_legacy_over_limit_users_and_history(pool: PgPool) {
    let mut baseline = sqlx::migrate!("../../migrations");
    baseline.migrations = Cow::Owned(
        baseline
            .iter()
            .filter(|migration| migration.version <= 20)
            .cloned()
            .collect(),
    );
    baseline.run(&pool).await.unwrap();
    let (app, admin, cookie) = fixture(&pool).await;
    fill(&pool, 12).await;
    insert_user(&pool, false).await;
    let before = snapshot(&pool).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    assert_eq!(
        snapshot(&pool).await,
        before,
        "Upgrade must not deactivate, delete or rewrite historical users, sessions or audit rows"
    );
    let listed = call(&app, &cookie, "GET", "/api/users", None).await;
    assert_eq!(
        listed["usage"],
        json!({"active":12,"limit":10,"available":0})
    );
    assert_eq!(listed["items"].as_array().unwrap().len(), 13);
    assert_eq!(
        request(&app, &cookie, "POST", "/api/users", Some(create_body()))
            .await
            .0,
        409
    );
    let users: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE active AND id<>$1 ORDER BY id LIMIT 3")
            .bind(admin)
            .fetch_all(&pool)
            .await
            .unwrap();
    // Ordinary updates remain available while legacy usage is above the cap.
    call(
        &app,
        &cookie,
        "PATCH",
        &user_path(users[0]),
        Some(json!({"role":"warehouse","active":true})),
    )
    .await;
    for id in users {
        call(
            &app,
            &cookie,
            "PATCH",
            &user_path(id),
            Some(json!({"active":false})),
        )
        .await;
    }
    assert_eq!(active(&pool).await, 9);
    call(&app, &cookie, "POST", "/api/users", Some(create_body())).await;
    assert_eq!(active(&pool).await, 10);
}
