use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use rand::RngCore;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct WeekBucket {
    pub label: String,
    pub temperature_mean: Option<f64>,
    pub temperature_std: Option<f64>,
    pub humidity_mean: Option<f64>,
    pub humidity_std: Option<f64>,
    pub wind_speed_mean: Option<f64>,
    pub wind_speed_std: Option<f64>,
    pub wind_direction_mean: Option<f64>,
    pub wind_direction_std: Option<f64>,
    pub luminosity_mean: Option<f64>,
    pub luminosity_std: Option<f64>,
    pub rainfall_mean: Option<f64>,
    pub rainfall_std: Option<f64>,
}

fn mean_and_std(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    Some((round2(mean), round2(variance.sqrt())))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const QUARTER_LABELS: [&str; 4] = ["0-6", "6-12", "12-18", "18-24"];

/// Given the Monday date string (e.g. "2025-01-13"), compute the bucket index
/// (0..28) for a reading's hour string (e.g. "2025-01-15T14").
/// Returns None if the reading doesn't belong to this week.
fn bucket_index(monday: &NaiveDate, hour_str: &str) -> Option<usize> {
    let date_str = &hour_str[..10];
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let day_offset = (date - *monday).num_days();
    if !(0..7).contains(&day_offset) {
        return None;
    }
    let hour: usize = hour_str[11..].parse().ok()?;
    let quarter = hour / 6;
    Some(day_offset as usize * 4 + quarter)
}

pub fn aggregate_week(monday: &str, readings: &[Reading]) -> Vec<WeekBucket> {
    let monday_date = NaiveDate::parse_from_str(monday, "%Y-%m-%d")
        .expect("invalid monday date");

    // Collect values per bucket per metric
    let mut buckets: Vec<[Vec<f64>; 6]> = (0..28)
        .map(|_| [vec![], vec![], vec![], vec![], vec![], vec![]])
        .collect();

    for r in readings {
        if let Some(idx) = bucket_index(&monday_date, &r.hour) {
            if let Some(v) = r.temperature { buckets[idx][0].push(v); }
            if let Some(v) = r.humidity { buckets[idx][1].push(v); }
            if let Some(v) = r.wind_speed { buckets[idx][2].push(v); }
            if let Some(v) = r.wind_direction { buckets[idx][3].push(v); }
            if let Some(v) = r.luminosity { buckets[idx][4].push(v); }
            if let Some(v) = r.rainfall { buckets[idx][5].push(v); }
        }
    }

    (0..28)
        .map(|i| {
            let day = i / 4;
            let quarter = i % 4;
            let label = format!("{} {}", DAY_NAMES[day], QUARTER_LABELS[quarter]);

            let temp = mean_and_std(&buckets[i][0]);
            let hum = mean_and_std(&buckets[i][1]);
            let wind = mean_and_std(&buckets[i][2]);
            let wdir = mean_and_std(&buckets[i][3]);
            let lux = mean_and_std(&buckets[i][4]);
            let rain = mean_and_std(&buckets[i][5]);

            WeekBucket {
                label,
                temperature_mean: temp.map(|t| t.0),
                temperature_std: temp.map(|t| t.1),
                humidity_mean: hum.map(|t| t.0),
                humidity_std: hum.map(|t| t.1),
                wind_speed_mean: wind.map(|t| t.0),
                wind_speed_std: wind.map(|t| t.1),
                wind_direction_mean: wdir.map(|t| t.0),
                wind_direction_std: wdir.map(|t| t.1),
                luminosity_mean: lux.map(|t| t.0),
                luminosity_std: lux.map(|t| t.1),
                rainfall_mean: rain.map(|t| t.0),
                rainfall_std: rain.map(|t| t.1),
            }
        })
        .collect()
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

pub async fn get_all_dates(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT DISTINCT substr(hour, 1, 10) FROM hourly_readings ORDER BY 1")
        .fetch_all(pool)
        .await
}

pub async fn generate_day_json(pool: &sqlx::SqlitePool, date: &str) -> Result<(), String> {
    let readings = get_readings_for_day(pool, date)
        .await
        .map_err(|e| e.to_string())?;

    let json = rocket::serde::json::serde_json::to_string(&readings)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all("data/static/day")
        .map_err(|e| e.to_string())?;

    std::fs::write(format!("data/static/day/{}.json", date), json)
        .map_err(|e| e.to_string())?;

    Ok(())
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
