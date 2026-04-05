use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::utils::io;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    http::client::{Configuration as HttpConfig, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    tls::X509,
};

mod lcd;
mod ntp;
mod sensor;
mod wifi;

const SERVER_URL: &str = env!("CFG_TORO_SERVER_URL");

// ISRG Root X1 — the Let's Encrypt root CA. NUL-terminated for mbedTLS.
const CA_CERT: X509<'static> =
    X509::pem_until_nul(include_bytes!("../certs/isrg-root-x1.pem"));

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let mut lcd = lcd::init(
        peripherals.i2c0,
        peripherals.pins.gpio2,
        peripherals.pins.gpio3,
    );

    let _reading = sensor::read(peripherals.pins.gpio10, &mut lcd);

    let _wifi = wifi::connect(peripherals.modem, sysloop, nvs, &mut lcd);

    ntp::sync(&mut lcd);

    // --- HTTPS GET ---
    let mut client = HttpClient::wrap(
        EspHttpConnection::new(&HttpConfig {
            server_certificate: Some(CA_CERT),
            ..Default::default()
        })
        .unwrap(),
    );

    let host = SERVER_URL
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    lcd::status(&mut lcd, "GET", host);
    log::info!("-> GET {}", SERVER_URL);

    let request = client.get(SERVER_URL).unwrap();
    let mut response = request.submit().unwrap();

    let status = response.status();
    log::info!("<- {}", status);
    lcd::status(&mut lcd, "HTTP", &status.to_string());

    let mut buf = [0u8; 1024];
    let bytes_read = io::try_read_full(&mut response, &mut buf)
        .map_err(|e| e.0)
        .unwrap();
    log::info!(
        "Body ({} bytes):\n{}",
        bytes_read,
        std::str::from_utf8(&buf[..bytes_read]).unwrap_or("<invalid utf8>")
    );

    lcd::status(&mut lcd, "OK", "");
    log::info!("BOOT_OK");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
