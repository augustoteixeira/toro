use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::{
    hal::delay::FreeRtos,
    http::client::{Configuration as HttpConfig, EspHttpConnection},
    tls::X509,
};

use crate::lcd::{self, Lcd};
use crate::sensor::Reading;

/// mm of rain per tipping bucket pulse.
const MM_PER_PULSE: f32 = 0.3;

const SERVER_URL: &str = env!("CFG_TORO_SERVER_URL");
const SERVER_TOKEN: &str = env!("CFG_TORO_SERVER_TOKEN");

// In test mode 1 real second = 360 simulated seconds (10 s real ≈ 1 h simulated).
// In normal mode time is not scaled.
#[cfg(feature = "test-mode")]
const TIME_SCALE: u64 = 360;
#[cfg(not(feature = "test-mode"))]
const TIME_SCALE: u64 = 1;

// How often to wake up and check whether a new scaled hour has arrived.
// In test mode check every second; in normal mode every 30 s is plenty.
#[cfg(feature = "test-mode")]
const POLL_MS: u32 = 1_000;
#[cfg(not(feature = "test-mode"))]
const POLL_MS: u32 = 30_000;

/// ISRG Root X1 — the Let's Encrypt root CA. NUL-terminated for mbedTLS.
pub const CA_CERT: X509<'static> =
    X509::pem_until_nul(include_bytes!("../certs/isrg-root-x1.pem"));

/// Format a Unix timestamp (in simulated seconds) as the server's hour key:
/// "YYYY-MM-DDTHH"  (no minutes, no timezone)
fn hour_key(unix_secs: u64) -> String {
    // Break unix_secs into calendar fields with integer arithmetic (UTC, no-std friendly).
    let secs_per_hour = 3600u64;
    let secs_per_day = 86_400u64;

    let hour = (unix_secs % secs_per_day) / secs_per_hour;

    // Days since Unix epoch (1970-01-01).
    let mut days = unix_secs / secs_per_day;

    // Gregorian calendar from day count.
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let mut month = 1u64;
    loop {
        let dim = days_in_month(month, year);
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}")
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(m: u64, y: u64) -> u64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => unreachable!(),
    }
}

/// Returns the current simulated Unix time in seconds.
/// boot_unix: real Unix time at boot (from NTP).
/// boot_instant: std::time::Instant at boot.
fn simulated_unix(boot_unix: u64, boot_instant: std::time::Instant) -> u64 {
    let elapsed = boot_instant.elapsed().as_secs();
    boot_unix + elapsed * TIME_SCALE
}

/// POST a single reading to the server. Returns the HTTP status code, or None
/// on a connection-level error (timeout, DNS failure, etc.).
fn post(
    client: &mut HttpClient<EspHttpConnection>,
    hour: &str,
    reading: &Reading,
    lcd: &mut Lcd<'_>,
) -> Option<u16> {
    let body = format!(
        r#"{{"hour":"{hour}","temperature":{:.1},"humidity":{:.1},"rainfall":{:.1}}}"#,
        reading.temperature, reading.humidity, reading.rainfall
    );

    let url = format!("{}readings", SERVER_URL);
    let headers = [
        ("Content-Type", "application/json"),
        ("Authorization", &format!("Bearer {SERVER_TOKEN}")),
    ];

    lcd::status(lcd, "POST", hour);
    log::info!("-> POST {} {}", url, body);

    let mut request = match client.post(&url, &headers) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("POST connect error: {e:?}");
            return None;
        }
    };
    if let Err(e) = request.write_all(body.as_bytes()) {
        log::warn!("POST write error: {e:?}");
        return None;
    }
    let response = match request.submit() {
        Ok(r) => r,
        Err(e) => {
            log::warn!("POST submit error: {e:?}");
            return None;
        }
    };

    let status = response.status();
    log::info!("<- {}", status);
    Some(status)
}

/// Run the main post loop forever.
/// Takes one reading per simulated hour and POSTs it to the server.
/// `boot_unix` is the Unix timestamp at boot, obtained from `ntp::fetch`.
/// `rain_counter` is an atomic count of tipping bucket pulses on GPIO4.
pub fn run_loop(
    mut reading: Reading,
    boot_unix: u64,
    rain_counter: Arc<AtomicU32>,
    lcd: &mut Lcd<'_>,
) -> ! {
    let boot_instant = std::time::Instant::now();

    let mut client = HttpClient::wrap(
        EspHttpConnection::new(&HttpConfig {
            server_certificate: Some(CA_CERT),
            ..Default::default()
        })
        .unwrap(),
    );

    let mut last_posted_hour = String::new();

    loop {
        let now_unix = simulated_unix(boot_unix, boot_instant);
        let current_hour = hour_key(now_unix);

        if current_hour != last_posted_hour {
            // Read and reset rain counter since last post.
            let pulses = rain_counter.swap(0, Ordering::Relaxed);
            reading.rainfall = pulses as f32 * MM_PER_PULSE;

            // Show current measurements on LCD before posting.
            lcd::status(
                lcd,
                &format!(
                    "{:.1}C {:.0}% {:.1}mm",
                    reading.temperature, reading.humidity, reading.rainfall
                ),
                &current_hour[5..],
            );

            match post(&mut client, &current_hour, &reading, lcd) {
                Some(201) => {
                    last_posted_hour = current_hour.clone();
                    lcd::status(lcd, &current_hour[5..], "Posted OK");
                }
                Some(422) => {
                    // Hour already exists in the DB — treat as success and move on.
                    log::info!("Hour {current_hour} already posted, skipping");
                    last_posted_hour = current_hour.clone();
                }
                Some(429) => {
                    // Rate limited — back off for 30 s before retrying.
                    log::warn!("Rate limited (429), backing off 30s");
                    lcd::status(lcd, "Rate limited", "Wait 30s...");
                    FreeRtos::delay_ms(30_000);
                }
                Some(status) => {
                    log::warn!(
                        "POST unexpected status {status}, skipping hour"
                    );
                    lcd::status(lcd, "POST failed", &status.to_string());
                    last_posted_hour = current_hour.clone();
                }
                None => {
                    log::warn!("POST connection error, will retry next poll");
                    lcd::status(lcd, "POST error", "Retrying...");
                }
            }
        }

        FreeRtos::delay_ms(POLL_MS);
    }
}
