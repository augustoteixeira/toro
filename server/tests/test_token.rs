mod common;

#[tokio::test]
async fn ensure_token_inserts_hash() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    server::ensure_token(&pool).await.expect("ensure_token failed");

    let hash: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'token_hash'")
            .fetch_one(&pool)
            .await
            .expect("token_hash not found");
    assert!(hash.starts_with("$2b$"), "expected bcrypt hash, got: {}", hash);
}

#[tokio::test]
async fn ensure_token_is_idempotent() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    server::ensure_token(&pool).await.expect("first call failed");

    let hash1: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'token_hash'")
            .fetch_one(&pool)
            .await
            .expect("token_hash not found");

    server::ensure_token(&pool).await.expect("second call failed");

    let hash2: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'token_hash'")
            .fetch_one(&pool)
            .await
            .expect("token_hash not found");

    assert_eq!(hash1, hash2, "token_hash changed on second call");
}
