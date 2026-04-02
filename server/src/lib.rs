use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::RngCore;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::serde::Deserialize;
use rocket::Request;
use rocket_db_pools::{Database, sqlx};

pub struct RateLimiter {
    attempts: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    pub fn too_many_attempts(&self, ip: IpAddr, limit: usize, window: Duration) -> bool {
        let mut attempts = self.attempts.lock().unwrap();
        let now = Instant::now();
        let timestamps = attempts.entry(ip).or_default();
        timestamps.retain(|&t| now.duration_since(t) < window);
        timestamps.push(now);
        timestamps.len() > limit
    }
}

#[derive(Database)]
#[database("db")]
pub struct Db(pub sqlx::SqlitePool);

pub async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let version: Option<String> =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'schema_version'")
            .fetch_optional(pool)
            .await?;

    match version.as_deref().unwrap_or("0") {
        "0" => {
            let sql = std::fs::read_to_string("migrations/001-init.sql")
                .expect("migrations/001-init.sql not found");
            sqlx::query(&sql).execute(pool).await?;
        }
        "1" => {}
        v => panic!("Unknown schema version: {}", v),
    }

    Ok(())
}

pub async fn ensure_token(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'token_hash'")
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        return Ok(());
    }

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);

    let hash = bcrypt::hash(&token, bcrypt::DEFAULT_COST)
        .expect("Failed to hash token");

    sqlx::query("INSERT INTO meta (key, value) VALUES ('token_hash', ?)")
        .bind(&hash)
        .execute(pool)
        .await?;

    println!("===========================================");
    println!("  API token (save this, shown only once):");
    println!("  {}", token);
    println!("===========================================");

    Ok(())
}

#[derive(Debug, Deserialize, sqlx::FromRow)]
#[serde(crate = "rocket::serde")]
pub struct Reading {
    pub hour: String,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<f64>,
    pub luminosity: Option<f64>,
    pub rainfall: Option<f64>,
}

pub async fn insert_reading(pool: &sqlx::SqlitePool, r: &Reading) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO hourly_readings (hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&r.hour)
    .bind(r.temperature)
    .bind(r.humidity)
    .bind(r.wind_speed)
    .bind(r.wind_direction)
    .bind(r.luminosity)
    .bind(r.rainfall)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_readings_for_day(
    pool: &sqlx::SqlitePool,
    date: &str,
) -> Result<Vec<Reading>, sqlx::Error> {
    let pattern = format!("{}%", date);
    sqlx::query_as::<_, Reading>(
        "SELECT hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall
         FROM hourly_readings WHERE hour LIKE ? ORDER BY hour",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await
}

pub struct TokenAuthenticated;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for TokenAuthenticated {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let pool = match req.rocket().state::<Db>() {
            Some(db) => &db.0,
            None => return Outcome::Error((Status::InternalServerError, ())),
        };

        let token = match req.headers().get_one("Authorization") {
            Some(header) if header.starts_with("Bearer ") => &header[7..],
            _ => return Outcome::Error((Status::Unauthorized, ())),
        };

        let hash: String = match sqlx::query_scalar(
            "SELECT value FROM meta WHERE key = 'token_hash'",
        )
        .fetch_optional(pool)
        .await
        {
            Ok(Some(h)) => h,
            _ => return Outcome::Error((Status::InternalServerError, ())),
        };

        match bcrypt::verify(token, &hash) {
            Ok(true) => Outcome::Success(TokenAuthenticated),
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}
