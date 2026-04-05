use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::utils::io;
use esp_idf_svc::http::client::{
    Configuration as HttpConfig, EspHttpConnection,
};

use crate::http::CA_CERT;
use crate::lcd::{self, Lcd};

const SERVER_URL: &str = env!("CFG_TORO_SERVER_URL");

/// Fetch the current UTC time from `GET /api/time` on the server.
///
/// The endpoint returns a JSON string: `"2026-04-05T14"`
/// We parse this into a Unix timestamp (seconds) for the start of that hour.
/// After this call, `SystemTime::now()` is not set — callers should use the
/// returned Unix timestamp directly rather than relying on the system clock.
pub fn fetch(lcd: &mut Lcd<'_>) -> u64 {
    lcd::status(lcd, "Time", "Fetching...");

    let url = format!("{}api/time", SERVER_URL);

    let mut client = HttpClient::wrap(
        EspHttpConnection::new(&HttpConfig {
            server_certificate: Some(CA_CERT),
            ..Default::default()
        })
        .unwrap(),
    );

    let mut response = client.get(&url).unwrap().submit().unwrap();
    assert_eq!(response.status(), 200, "GET /api/time returned non-200");

    let mut buf = [0u8; 32];
    let n = io::try_read_full(&mut response, &mut buf)
        .map_err(|e| e.0)
        .unwrap();

    // Response is: "2026-04-05T14"  (with surrounding quotes)
    let body = std::str::from_utf8(&buf[..n]).unwrap().trim();
    let hour_key = body.trim_matches('"');
    log::info!("Server time: {}", hour_key);

    let unix = hour_key_to_unix(hour_key);
    lcd::status(lcd, "Time", &hour_key[5..]); // show MM-DDTHH
    unix
}

/// Parse `"YYYY-MM-DDTHH"` into a Unix timestamp for the start of that hour.
fn hour_key_to_unix(s: &str) -> u64 {
    // s = "2026-04-05T14"
    let year: u64 = s[0..4].parse().unwrap();
    let month: u64 = s[5..7].parse().unwrap();
    let day: u64 = s[8..10].parse().unwrap();
    let hour: u64 = s[11..13].parse().unwrap();

    // Days since Unix epoch via Gregorian calendar.
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month(m, year);
    }
    days += day - 1;

    days * 86_400 + hour * 3_600
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
