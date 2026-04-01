mod common;

use sqlx::Row;

#[tokio::test]
async fn migrate_creates_tables_and_sets_version() {
    let pool = common::test_pool().await;

    server::migrate(&pool).await.expect("migration failed");

    // schema_version should be "1"
    let version: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'schema_version'")
            .fetch_one(&pool)
            .await
            .expect("schema_version not found");
    assert_eq!(version, "1");

    // hourly_readings table should exist
    let table_exists: i32 = sqlx::query(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'hourly_readings'",
    )
    .fetch_one(&pool)
    .await
    .expect("sqlite_master query failed")
    .get(0);
    assert_eq!(table_exists, 1);
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let pool = common::test_pool().await;

    server::migrate(&pool).await.expect("first migration failed");
    server::migrate(&pool).await.expect("second migration failed");

    let version: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'schema_version'")
            .fetch_one(&pool)
            .await
            .expect("schema_version not found");
    assert_eq!(version, "1");
}
