mod common;

use rocket::http::{ContentType, Header, Status};
use rocket::local::asynchronous::Client;
use server::{Db, Reading, TokenAuthenticated, generate_day_json, insert_reading};
use sqlx::Row;

#[rocket::post("/readings", data = "<reading>")]
async fn post_reading(
    _auth: TokenAuthenticated,
    db: &rocket::State<Db>,
    reading: rocket::serde::json::Json<Reading>,
) -> Status {
    let reading = reading.into_inner();
    let date = reading.hour.chars().take(10).collect::<String>();
    match insert_reading(&db.0, &reading).await {
        Ok(_) => {
            let _ = generate_day_json(&db.0, &date).await;
            Status::Created
        }
        Err(_) => Status::UnprocessableEntity,
    }
}

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

async fn setup() -> (Client, String) {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");
    let token = seed_token(&pool).await;

    let rocket = rocket::custom(rocket::Config::figment())
        .manage(Db(pool))
        .mount("/", rocket::routes![post_reading]);

    let client = Client::tracked(rocket).await.expect("valid rocket");
    (client, token)
}

#[tokio::test]
async fn insert_reading_stores_row() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    let reading = Reading {
        hour: "2026-03-15T14".to_string(),
        temperature: Some(28.5),
        humidity: Some(46.0),
        wind_speed: Some(10.2),
        wind_direction: Some(110.0),
        luminosity: Some(880.0),
        rainfall: Some(0.0),
    };

    insert_reading(&pool, &reading).await.expect("insert failed");

    let count: i32 = sqlx::query("SELECT count(*) FROM hourly_readings")
        .fetch_one(&pool)
        .await
        .expect("count query failed")
        .get(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn post_with_valid_token_inserts_row() {
    let (client, token) = setup().await;

    let response = client
        .post("/readings")
        .header(ContentType::JSON)
        .header(Header::new("Authorization", format!("Bearer {}", token)))
        .body(r#"{
            "hour": "2026-03-15T14",
            "temperature": 28.5,
            "humidity": 46.0,
            "wind_speed": 10.2,
            "wind_direction": 110.0,
            "luminosity": 880.0,
            "rainfall": 0.0
        }"#)
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Created);
}

#[tokio::test]
async fn post_generates_day_json() {
    let (client, token) = setup().await;

    let response = client
        .post("/readings")
        .header(ContentType::JSON)
        .header(Header::new("Authorization", format!("Bearer {}", token)))
        .body(r#"{
            "hour": "2026-04-01T12",
            "temperature": 22.0,
            "humidity": 55.0,
            "wind_speed": 6.0,
            "wind_direction": 180.0,
            "luminosity": 500.0,
            "rainfall": 0.0
        }"#)
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Created);

    let path = "data/static/day/2026-04-01.json";
    let contents = std::fs::read_to_string(path).expect("JSON file not generated after POST");
    assert!(contents.contains("2026-04-01T12"));
    assert!(contents.contains("22.0"));

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn post_without_token_is_rejected() {
    let (client, _token) = setup().await;

    let response = client
        .post("/readings")
        .header(ContentType::JSON)
        .body(r#"{
            "hour": "2026-03-15T14",
            "temperature": 28.5
        }"#)
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Unauthorized);
}

#[tokio::test]
async fn post_duplicate_hour_returns_error() {
    let (client, token) = setup().await;

    let body = r#"{
        "hour": "2026-03-15T14",
        "temperature": 28.5,
        "humidity": 46.0,
        "wind_speed": 10.2,
        "wind_direction": 110.0,
        "luminosity": 880.0,
        "rainfall": 0.0
    }"#;

    let auth = Header::new("Authorization", format!("Bearer {}", token));

    let first = client
        .post("/readings")
        .header(ContentType::JSON)
        .header(auth.clone())
        .body(body)
        .dispatch()
        .await;
    assert_eq!(first.status(), Status::Created);

    let second = client
        .post("/readings")
        .header(ContentType::JSON)
        .header(Header::new("Authorization", format!("Bearer {}", token)))
        .body(body)
        .dispatch()
        .await;
    assert_eq!(second.status(), Status::UnprocessableEntity);
}
