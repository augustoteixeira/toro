use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::utils::io;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        delay::FreeRtos,
        i2c::{I2cConfig, I2cDriver},
        peripherals::Peripherals,
        units::Hertz,
    },
    http::client::{Configuration as HttpConfig, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    tls::X509,
    wifi::{
        AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
    },
};
use i2c_character_display::{CharacterDisplayPCF8574T, LcdDisplayType};

const WIFI_SSID: &str = env!("CFG_TORO_WIFI_SSID");
const WIFI_PASSWORD: &str = env!("CFG_TORO_WIFI_PASSWORD");
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

    // --- LCD via I2C ---
    let sda = peripherals.pins.gpio2;
    let scl = peripherals.pins.gpio3;
    let i2c_config = I2cConfig::new().baudrate(Hertz(100_000));
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &i2c_config).unwrap();

    let mut lcd =
        CharacterDisplayPCF8574T::new(i2c, LcdDisplayType::Lcd16x2, FreeRtos);
    lcd.init().map_err(|_| "LCD init failed").unwrap();
    lcd.set_cursor(0, 0)
        .map_err(|_| "LCD set_cursor failed")
        .unwrap();
    lcd.print("Hello, Toro!")
        .map_err(|_| "LCD print failed")
        .unwrap();

    log::info!("LCD initialised");

    // --- Wi-Fi ---
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs)).unwrap(),
        sysloop,
    )
    .unwrap();

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASSWORD.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))
    .unwrap();

    wifi.start().unwrap();
    log::info!("Wi-Fi started, connecting to '{}'…", WIFI_SSID);

    wifi.connect().unwrap();
    log::info!("Connected, waiting for IP…");

    wifi.wait_netif_up().unwrap();

    let ip = wifi.wifi().sta_netif().get_ip_info().unwrap();
    log::info!("IP address: {}", ip.ip);

    // --- HTTPS GET ---
    let mut client = HttpClient::wrap(
        EspHttpConnection::new(&HttpConfig {
            server_certificate: Some(CA_CERT),
            ..Default::default()
        })
        .unwrap(),
    );

    log::info!("-> GET {}", SERVER_URL);
    let request = client.get(SERVER_URL).unwrap();
    let mut response = request.submit().unwrap();

    let status = response.status();
    log::info!("<- {}", status);

    let mut buf = [0u8; 1024];
    let bytes_read = io::try_read_full(&mut response, &mut buf)
        .map_err(|e| e.0)
        .unwrap();
    log::info!(
        "Body ({} bytes):\n{}",
        bytes_read,
        std::str::from_utf8(&buf[..bytes_read]).unwrap_or("<invalid utf8>")
    );

    log::info!("BOOT_OK");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
