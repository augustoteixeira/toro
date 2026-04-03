use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate};
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
pub struct AggregatedBucket {
    pub label: String,
    pub temperature_mean: Option<f64>,
    pub temperature_std: Option<f64>,
    pub humidity_mean: Option<f64>,
    pub humidity_std: Option<f64>,
    pub wind_speed_mean: Option<f64>,
    pub wind_speed_std: Option<f64>,
    pub wind_direction_mean: Option<f64>,
    pub luminosity_mean: Option<f64>,
    pub luminosity_std: Option<f64>,
    pub rainfall_sum: Option<f64>,
    pub rainfall_max: Option<f64>,
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

/// Average wind direction using vector decomposition.
/// Each (speed, direction) pair is converted to Cartesian, averaged, then
/// converted back to an angle in degrees.
fn vector_mean_direction(speeds: &[f64], directions: &[f64]) -> Option<f64> {
    if speeds.is_empty() || speeds.len() != directions.len() {
        return None;
    }
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for (s, d) in speeds.iter().zip(directions.iter()) {
        let rad = d.to_radians();
        sum_x += s * rad.cos();
        sum_y += s * rad.sin();
    }
    let n = speeds.len() as f64;
    let avg_x = sum_x / n;
    let avg_y = sum_y / n;
    let angle = avg_y.atan2(avg_x).to_degrees();
    Some(round2(if angle < 0.0 { angle + 360.0 } else { angle }))
}

fn sum_and_max(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let sum = values.iter().sum::<f64>();
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Some((round2(sum), round2(max)))
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

struct BucketCollector {
    temperature: Vec<f64>,
    humidity: Vec<f64>,
    wind_speed: Vec<f64>,
    wind_direction: Vec<f64>,
    /// Wind speed values paired with wind_direction (only when both are present)
    wind_speed_for_dir: Vec<f64>,
    luminosity: Vec<f64>,
    rainfall: Vec<f64>,
}

impl BucketCollector {
    fn new() -> Self {
        Self {
            temperature: vec![],
            humidity: vec![],
            wind_speed: vec![],
            wind_direction: vec![],
            wind_speed_for_dir: vec![],
            luminosity: vec![],
            rainfall: vec![],
        }
    }
}

fn collector_to_bucket(label: String, b: &BucketCollector) -> AggregatedBucket {
    let temp = mean_and_std(&b.temperature);
    let hum  = mean_and_std(&b.humidity);
    let wind = mean_and_std(&b.wind_speed);
    let wdir = vector_mean_direction(&b.wind_speed_for_dir, &b.wind_direction);
    let lux  = mean_and_std(&b.luminosity);
    let rain = sum_and_max(&b.rainfall);
    AggregatedBucket {
        label,
        temperature_mean:    temp.map(|t| t.0),
        temperature_std:     temp.map(|t| t.1),
        humidity_mean:       hum.map(|t| t.0),
        humidity_std:        hum.map(|t| t.1),
        wind_speed_mean:     wind.map(|t| t.0),
        wind_speed_std:      wind.map(|t| t.1),
        wind_direction_mean: wdir,
        luminosity_mean:     lux.map(|t| t.0),
        luminosity_std:      lux.map(|t| t.1),
        rainfall_sum:        rain.map(|t| t.0),
        rainfall_max:        rain.map(|t| t.1),
    }
}

pub fn aggregate_week(monday: &str, readings: &[Reading]) -> Vec<AggregatedBucket> {
    let monday_date = NaiveDate::parse_from_str(monday, "%Y-%m-%d")
        .expect("invalid monday date");

    let mut buckets: Vec<BucketCollector> = (0..28).map(|_| BucketCollector::new()).collect();

    for r in readings {
        if let Some(idx) = bucket_index(&monday_date, &r.hour) {
            let b = &mut buckets[idx];
            if let Some(v) = r.temperature { b.temperature.push(v); }
            if let Some(v) = r.humidity { b.humidity.push(v); }
            if let Some(v) = r.wind_speed { b.wind_speed.push(v); }
            if let (Some(s), Some(d)) = (r.wind_speed, r.wind_direction) {
                b.wind_speed_for_dir.push(s);
                b.wind_direction.push(d);
            }
            if let Some(v) = r.luminosity { b.luminosity.push(v); }
            if let Some(v) = r.rainfall { b.rainfall.push(v); }
        }
    }

    (0..28)
        .map(|i| {
            let day = i / 4;
            let quarter = i % 4;
            let label = format!("{} {}", DAY_NAMES[day], QUARTER_LABELS[quarter]);
            collector_to_bucket(label, &buckets[i])
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

pub async fn get_readings_for_week(
    pool: &sqlx::SqlitePool,
    monday: &str,
) -> Result<Vec<Reading>, sqlx::Error> {
    let monday_date = NaiveDate::parse_from_str(monday, "%Y-%m-%d")
        .expect("invalid monday date");
    let sunday = monday_date + chrono::Duration::days(6);
    let start = format!("{}T00", monday);
    let end = format!("{}T23", sunday);
    sqlx::query_as::<_, Reading>(
        "SELECT hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall
         FROM hourly_readings WHERE hour >= ? AND hour <= ? ORDER BY hour",
    )
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
}

pub async fn get_all_dates(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT DISTINCT substr(hour, 1, 10) FROM hourly_readings ORDER BY 1")
        .fetch_all(pool)
        .await
}

/// Returns distinct Monday dates for all weeks that have at least one reading.
pub async fn get_all_weeks(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let dates: Vec<String> = get_all_dates(pool).await?;
    let mut mondays: Vec<String> = dates
        .iter()
        .filter_map(|d| {
            let date = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
            let weekday = date.weekday().num_days_from_monday(); // 0=Mon
            let monday = date - chrono::Duration::days(weekday as i64);
            Some(monday.format("%Y-%m-%d").to_string())
        })
        .collect();
    mondays.dedup();
    Ok(mondays)
}

/// Given a reading's hour string, compute the Monday of its week.
pub fn monday_of(hour: &str) -> String {
    let date = NaiveDate::parse_from_str(&hour[..10], "%Y-%m-%d")
        .expect("invalid date in hour string");
    let weekday = date.weekday().num_days_from_monday();
    let monday = date - chrono::Duration::days(weekday as i64);
    monday.format("%Y-%m-%d").to_string()
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

pub async fn generate_week_json(pool: &sqlx::SqlitePool, monday: &str) -> Result<(), String> {
    let readings = get_readings_for_week(pool, monday)
        .await
        .map_err(|e| e.to_string())?;

    let buckets = aggregate_week(monday, &readings);

    let json = rocket::serde::json::serde_json::to_string(&buckets)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all("data/static/week")
        .map_err(|e| e.to_string())?;

    std::fs::write(format!("data/static/week/{}.json", monday), json)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_readings_for_month(
    pool: &sqlx::SqlitePool,
    month: &str,
) -> Result<Vec<Reading>, sqlx::Error> {
    let pattern = format!("{}%", month);
    sqlx::query_as::<_, Reading>(
        "SELECT hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall
         FROM hourly_readings WHERE hour LIKE ? ORDER BY hour",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await
}

pub fn aggregate_month(month: &str, readings: &[Reading]) -> Vec<AggregatedBucket> {
    // Determine number of days in month
    let year: i32 = month[..4].parse().expect("invalid year");
    let mo: u32 = month[5..7].parse().expect("invalid month");
    let days_in_month = NaiveDate::from_ymd_opt(
        if mo == 12 { year + 1 } else { year },
        if mo == 12 { 1 } else { mo + 1 },
        1,
    )
    .unwrap()
    .signed_duration_since(NaiveDate::from_ymd_opt(year, mo, 1).unwrap())
    .num_days() as usize;

    let mut buckets: Vec<BucketCollector> =
        (0..days_in_month).map(|_| BucketCollector::new()).collect();

    for r in readings {
        let date_str = &r.hour[..10];
        let day: usize = date_str[8..10].parse::<usize>().unwrap_or(0);
        if day >= 1 && day <= days_in_month {
            let b = &mut buckets[day - 1];
            if let Some(v) = r.temperature { b.temperature.push(v); }
            if let Some(v) = r.humidity { b.humidity.push(v); }
            if let Some(v) = r.wind_speed { b.wind_speed.push(v); }
            if let (Some(s), Some(d)) = (r.wind_speed, r.wind_direction) {
                b.wind_speed_for_dir.push(s);
                b.wind_direction.push(d);
            }
            if let Some(v) = r.luminosity { b.luminosity.push(v); }
            if let Some(v) = r.rainfall { b.rainfall.push(v); }
        }
    }

    (0..days_in_month)
        .map(|i| {
            let label = format!("{}-{:02}", month, i + 1);
            collector_to_bucket(label, &buckets[i])
        })
        .collect()
}

pub async fn generate_month_json(pool: &sqlx::SqlitePool, month: &str) -> Result<(), String> {
    let readings = get_readings_for_month(pool, month)
        .await
        .map_err(|e| e.to_string())?;

    let buckets = aggregate_month(month, &readings);

    let json = rocket::serde::json::serde_json::to_string(&buckets)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all("data/static/month")
        .map_err(|e| e.to_string())?;

    std::fs::write(format!("data/static/month/{}.json", month), json)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_all_months(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT substr(hour, 1, 7) FROM hourly_readings ORDER BY 1",
    )
    .fetch_all(pool)
    .await
}

/// Given a reading's hour string, return its month key e.g. "2025-01".
pub fn month_of(hour: &str) -> String {
    hour[..7].to_string()
}

const SEMESTER_WEEKS: usize = 26;

pub async fn get_readings_for_semester(
    pool: &sqlx::SqlitePool,
    start_monday: &str,
) -> Result<Vec<Reading>, sqlx::Error> {
    let start = NaiveDate::parse_from_str(start_monday, "%Y-%m-%d")
        .expect("invalid start monday");
    let end = start + chrono::Duration::weeks(SEMESTER_WEEKS as i64) - chrono::Duration::days(1);
    let start_str = format!("{}T00", start_monday);
    let end_str = format!("{}T23", end.format("%Y-%m-%d"));
    sqlx::query_as::<_, Reading>(
        "SELECT hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall
         FROM hourly_readings WHERE hour >= ? AND hour <= ? ORDER BY hour",
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
}

pub fn aggregate_semester(start_monday: &str, readings: &[Reading]) -> Vec<AggregatedBucket> {
    let start = NaiveDate::parse_from_str(start_monday, "%Y-%m-%d")
        .expect("invalid start monday");

    let mut buckets: Vec<BucketCollector> =
        (0..SEMESTER_WEEKS).map(|_| BucketCollector::new()).collect();

    for r in readings {
        let date = NaiveDate::parse_from_str(&r.hour[..10], "%Y-%m-%d").unwrap();
        let day_offset = (date - start).num_days();
        if day_offset < 0 {
            continue;
        }
        let week_offset = day_offset / 7;
        if (week_offset as usize) < SEMESTER_WEEKS {
            let b = &mut buckets[week_offset as usize];
            if let Some(v) = r.temperature { b.temperature.push(v); }
            if let Some(v) = r.humidity { b.humidity.push(v); }
            if let Some(v) = r.wind_speed { b.wind_speed.push(v); }
            if let (Some(s), Some(d)) = (r.wind_speed, r.wind_direction) {
                b.wind_speed_for_dir.push(s);
                b.wind_direction.push(d);
            }
            if let Some(v) = r.luminosity { b.luminosity.push(v); }
            if let Some(v) = r.rainfall { b.rainfall.push(v); }
        }
    }

    (0..SEMESTER_WEEKS)
        .map(|i| {
            let week_start = start + chrono::Duration::weeks(i as i64);
            let label = week_start.format("%Y-%m-%d").to_string();
            collector_to_bucket(label, &buckets[i])
        })
        .collect()
}

pub async fn generate_semester_json(
    pool: &sqlx::SqlitePool,
    start_monday: &str,
) -> Result<(), String> {
    let readings = get_readings_for_semester(pool, start_monday)
        .await
        .map_err(|e| e.to_string())?;

    let buckets = aggregate_semester(start_monday, &readings);

    let json = rocket::serde::json::serde_json::to_string(&buckets)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all("data/static/semester")
        .map_err(|e| e.to_string())?;

    std::fs::write(format!("data/static/semester/{}.json", start_monday), json)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Returns all distinct semester start Mondays (every 26 weeks) that overlap with the data.
pub async fn get_all_semesters(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let dates: Vec<String> = get_all_dates(pool).await?;
    if dates.is_empty() {
        return Ok(vec![]);
    }
    // Find the earliest Monday in the data
    let first = NaiveDate::parse_from_str(&dates[0], "%Y-%m-%d").unwrap();
    let first_monday = first - chrono::Duration::days(first.weekday().num_days_from_monday() as i64);
    let last = NaiveDate::parse_from_str(dates.last().unwrap(), "%Y-%m-%d").unwrap();

    let mut semesters = vec![];
    let mut cursor = first_monday;
    while cursor <= last {
        semesters.push(cursor.format("%Y-%m-%d").to_string());
        cursor += chrono::Duration::weeks(SEMESTER_WEEKS as i64);
    }
    Ok(semesters)
}

/// Given a reading's hour string, return the semester start Monday it belongs to.
pub fn semester_start_of(hour: &str) -> String {
    let date = NaiveDate::parse_from_str(&hour[..10], "%Y-%m-%d")
        .expect("invalid date in hour string");
    let monday = date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64);
    // Find how many 26-week periods since the Unix epoch Monday (1970-01-05)
    let epoch_monday = NaiveDate::from_ymd_opt(1970, 1, 5).unwrap();
    let weeks_since_epoch = (monday - epoch_monday).num_weeks();
    let semester_index = weeks_since_epoch.div_euclid(SEMESTER_WEEKS as i64);
    let start = epoch_monday + chrono::Duration::weeks(semester_index * SEMESTER_WEEKS as i64);
    start.format("%Y-%m-%d").to_string()
}

const TRIENNIUM_MONTHS: usize = 36;

pub async fn get_readings_for_triennium(
    pool: &sqlx::SqlitePool,
    start: &str,
) -> Result<Vec<Reading>, sqlx::Error> {
    // start is "YYYY-MM-01"
    let start_year: i32 = start[..4].parse().expect("invalid year");
    let start_mo: u32 = start[5..7].parse().expect("invalid month");
    // Compute end: start + 36 months - 1 day
    let total_months = start_mo as i32 - 1 + TRIENNIUM_MONTHS as i32;
    let end_year = start_year + total_months / 12;
    let end_mo = (total_months % 12 + 1) as u32;
    // Last day of the month before end
    let end_date = NaiveDate::from_ymd_opt(
        if end_mo == 1 { end_year - 1 } else { end_year },
        if end_mo == 1 { 12 } else { end_mo - 1 },
        1,
    )
    .unwrap();
    let days_in_end_month = NaiveDate::from_ymd_opt(end_year, end_mo, 1)
        .unwrap()
        .signed_duration_since(end_date)
        .num_days() as u32;
    let end_day = days_in_end_month;
    let end_str = format!(
        "{}-{:02}-{:02}T23",
        end_date.year(), end_date.month(), end_day
    );
    let start_str = format!("{}T00", start);
    sqlx::query_as::<_, Reading>(
        "SELECT hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall
         FROM hourly_readings WHERE hour >= ? AND hour <= ? ORDER BY hour",
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
}

pub fn aggregate_triennium(start: &str, readings: &[Reading]) -> Vec<AggregatedBucket> {
    let start_year: i32 = start[..4].parse().expect("invalid year");
    let start_mo: u32 = start[5..7].parse().expect("invalid month");

    let mut buckets: Vec<BucketCollector> =
        (0..TRIENNIUM_MONTHS).map(|_| BucketCollector::new()).collect();

    for r in readings {
        let year: i32 = r.hour[..4].parse().unwrap();
        let mo: u32 = r.hour[5..7].parse().unwrap();
        let month_offset = (year - start_year) * 12 + mo as i32 - start_mo as i32;
        if month_offset >= 0 && (month_offset as usize) < TRIENNIUM_MONTHS {
            let b = &mut buckets[month_offset as usize];
            if let Some(v) = r.temperature { b.temperature.push(v); }
            if let Some(v) = r.humidity { b.humidity.push(v); }
            if let Some(v) = r.wind_speed { b.wind_speed.push(v); }
            if let (Some(s), Some(d)) = (r.wind_speed, r.wind_direction) {
                b.wind_speed_for_dir.push(s);
                b.wind_direction.push(d);
            }
            if let Some(v) = r.luminosity { b.luminosity.push(v); }
            if let Some(v) = r.rainfall { b.rainfall.push(v); }
        }
    }

    (0..TRIENNIUM_MONTHS)
        .map(|i| {
            let total_mo = start_mo as i32 - 1 + i as i32;
            let year = start_year + total_mo / 12;
            let mo = (total_mo % 12 + 1) as u32;
            let label = format!("{}-{:02}", year, mo);
            collector_to_bucket(label, &buckets[i])
        })
        .collect()
}

pub async fn generate_triennium_json(
    pool: &sqlx::SqlitePool,
    start: &str,
) -> Result<(), String> {
    let readings = get_readings_for_triennium(pool, start)
        .await
        .map_err(|e| e.to_string())?;

    let buckets = aggregate_triennium(start, &readings);

    let json = rocket::serde::json::serde_json::to_string(&buckets)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all("data/static/triennium")
        .map_err(|e| e.to_string())?;

    std::fs::write(format!("data/static/triennium/{}.json", start), json)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_all_triennia(pool: &sqlx::SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let dates: Vec<String> = get_all_dates(pool).await?;
    if dates.is_empty() {
        return Ok(vec![]);
    }
    let first = NaiveDate::parse_from_str(&dates[0], "%Y-%m-%d").unwrap();
    let last = NaiveDate::parse_from_str(dates.last().unwrap(), "%Y-%m-%d").unwrap();

    let mut triennia = vec![];
    let mut year = first.year();
    let mut mo = first.month();
    // Snap to first of month
    loop {
        let start = NaiveDate::from_ymd_opt(year, mo, 1).unwrap();
        triennia.push(start.format("%Y-%m-%d").to_string());
        // Advance 36 months
        let total = mo as i32 - 1 + TRIENNIUM_MONTHS as i32;
        year += total / 12;
        mo = (total % 12 + 1) as u32;
        if NaiveDate::from_ymd_opt(year, mo, 1).unwrap() > last {
            break;
        }
    }
    Ok(triennia)
}

/// Given a reading's hour string, return the triennium start ("YYYY-MM-01") it belongs to.
pub fn triennium_start_of(hour: &str) -> String {
    let year: i32 = hour[..4].parse().expect("invalid year");
    let mo: u32 = hour[5..7].parse().expect("invalid month");
    // Months since epoch start (1970-01)
    let months_since_epoch = (year - 1970) * 12 + mo as i32 - 1;
    let tri_index = months_since_epoch.div_euclid(TRIENNIUM_MONTHS as i32);
    let start_months = tri_index * TRIENNIUM_MONTHS as i32;
    let start_year = 1970 + start_months / 12;
    let start_mo = (start_months % 12 + 1) as u32;
    format!("{}-{:02}-01", start_year, start_mo)
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
