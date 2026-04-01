use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

pub async fn test_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("Failed to create in-memory SQLite pool")
}
