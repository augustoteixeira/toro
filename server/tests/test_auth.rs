mod common;

use rocket::http::{Header, Status};
use rocket::local::asynchronous::Client;
use server::{Db, TokenAuthenticated};

#[rocket::get("/protected")]
fn protected(_auth: TokenAuthenticated) -> &'static str {
    "ok"
}

/// Store a known token hash in the meta table and return the plaintext token.
async fn seed_token(pool: &sqlx::SqlitePool) -> String {
    let token = "test-token-abc123";
    let hash = bcrypt::hash(token, bcrypt::DEFAULT_COST).expect("hash failed");
    sqlx::query("INSERT INTO meta (key, value) VALUES ('token_hash', ?)")
        .bind(&hash)
        .execute(pool)
        .await
        .expect("insert token_hash failed");
    token.to_string()
}

#[tokio::test]
async fn valid_token_is_accepted() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");
    let token = seed_token(&pool).await;

    // Build a Rocket instance that uses this specific pool
    let figment = rocket::Config::figment()
        .merge(("databases.db.url", format!("sqlite::memory:")));

    let rocket = rocket::custom(figment)
        .manage(Db(pool))
        .mount("/", rocket::routes![protected]);

    let client = Client::tracked(rocket).await.expect("valid rocket");

    let response = client
        .get("/protected")
        .header(Header::new("Authorization", format!("Bearer {}", token)))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.into_string().await.unwrap(), "ok");
}

#[tokio::test]
async fn missing_token_is_rejected() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");
    seed_token(&pool).await;

    let rocket = rocket::custom(rocket::Config::figment())
        .manage(Db(pool))
        .mount("/", rocket::routes![protected]);

    let client = Client::tracked(rocket).await.expect("valid rocket");

    let response = client.get("/protected").dispatch().await;

    assert_eq!(response.status(), Status::Unauthorized);
}

#[tokio::test]
async fn wrong_token_is_rejected() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");
    seed_token(&pool).await;

    let rocket = rocket::custom(rocket::Config::figment())
        .manage(Db(pool))
        .mount("/", rocket::routes![protected]);

    let client = Client::tracked(rocket).await.expect("valid rocket");

    let response = client
        .get("/protected")
        .header(Header::new("Authorization", "Bearer wrong-token"))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Unauthorized);
}
