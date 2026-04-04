use dht_embedded::{Dht22, DhtSensor, NoopInterruptControl};
use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::utils::io;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        delay::{Ets, FreeRtos},
        gpio::{PinDriver, Pull},
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

type Lcd<'d> = CharacterDisplayPCF8574T<I2cDriver<'d>, FreeRtos>;

/// Write a two-line status to the LCD. Row 0 is a short label, row 1 is a value.
/// Both lines are padded with spaces to 16 chars to erase any previous content.
fn lcd_status(lcd: &mut Lcd<'_>, label: &str, value: &str) {
    let label = format!("{:<16}", &label[..label.len().min(16)]);
    let value = format!("{:<16}", &value[..value.len().min(16)]);
    lcd.set_cursor(0, 0).map_err(|_| ()).unwrap();
    lcd.print(&label).map_err(|_| ()).unwrap();
    lcd.set_cursor(0, 1).map_err(|_| ()).unwrap();
    lcd.print(&value).map_err(|_| ()).unwrap();
}

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
    log::info!("LCD initialised");

    // --- DHT22 on GPIO10 ---
    lcd_status(&mut lcd, "DHT22", "Reading...");

    // Open-drain mode: the pin can be driven low by us or the sensor, and the
    // external 4.7 kΩ pull-up brings it high when neither is pulling it down.
    let mut dht_pin =
        PinDriver::input_output_od(peripherals.pins.gpio10, Pull::Floating)
            .unwrap();
    dht_pin.set_high().unwrap();

    // Sanity check: with the external pull-up the idle line must read high.
    // If it reads low the pull-up resistor is missing or wired incorrectly.
    FreeRtos::delay_ms(10);
    if !dht_pin.is_high() {
        log::error!(
            "DHT22 GPIO10 reads LOW at idle — check pull-up resistor wiring"
        );
    }

    // Ets uses a busy-wait spin loop accurate to 1 µs — required for DHT22's
    // tight timing. FreeRtos has only 1 ms tick resolution and corrupts the protocol.
    let mut sensor = Dht22::new(NoopInterruptControl, Ets, dht_pin);

    // DHT22 needs ~1 s after power-on before the first read is valid.
    FreeRtos::delay_ms(1500);

    for i in 1..=5 {
        match sensor.read() {
            Ok(reading) => {
                log::info!(
                    "DHT22 [{i}/5]: {:.1} °C  {:.1} %RH",
                    reading.temperature(),
                    reading.humidity()
                );
                let line = format!(
                    "{:.1}C  {:.1}%",
                    reading.temperature(),
                    reading.humidity()
                );
                lcd_status(&mut lcd, "Temp / Humidity", &line);
            }
            Err(e) => {
                log::warn!("DHT22 [{i}/5] error: {:?}", e);
                lcd_status(&mut lcd, "DHT22 error", &format!("{:?}", e));
            }
        }
        // DHT22 requires at least 2 s between reads.
        if i < 5 {
            FreeRtos::delay_ms(2500);
        }
    }

    // --- Wi-Fi ---
    lcd_status(&mut lcd, "Wi-Fi", "Connecting...");

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

    // Scan and log all visible networks.
    log::info!("Scanning for Wi-Fi networks...");
    let aps = wifi.wifi_mut().scan().unwrap();
    log::info!("Found {} networks:", aps.len());
    for ap in &aps {
        log::info!(
            "  SSID: {:?}  RSSI: {} dBm  Auth: {:?}  Channel: {}",
            ap.ssid,
            ap.signal_strength,
            ap.auth_method,
            ap.channel
        );
    }

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASSWORD.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))
    .unwrap();
    wifi.stop().unwrap();
    wifi.start().unwrap();

    // Retry connect — the AP may take a moment to become visible.
    let mut connected = false;
    for attempt in 1..=5 {
        match wifi.connect() {
            Ok(_) => {
                connected = true;
                break;
            }
            Err(e) => {
                log::warn!("Wi-Fi connect attempt {attempt}/5 failed: {e:?}");
                lcd_status(&mut lcd, "Wi-Fi", &format!("Retry {attempt}/5..."));
                FreeRtos::delay_ms(2000);
            }
        }
    }
    if !connected {
        lcd_status(&mut lcd, "Wi-Fi FAILED", "Check credentials");
        panic!("Wi-Fi: all connect attempts failed");
    }
    log::info!("Wi-Fi connected, waiting for IP…");

    lcd_status(&mut lcd, "Wi-Fi", "Getting IP...");
    wifi.wait_netif_up().unwrap();

    let ip = wifi.wifi().sta_netif().get_ip_info().unwrap();
    let ip_str = ip.ip.to_string();
    log::info!("IP address: {}", ip_str);
    lcd_status(&mut lcd, "IP:", &ip_str);

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
    lcd_status(&mut lcd, "GET", host);
    log::info!("-> GET {}", SERVER_URL);

    let request = client.get(SERVER_URL).unwrap();
    let mut response = request.submit().unwrap();

    let status = response.status();
    log::info!("<- {}", status);
    lcd_status(&mut lcd, "HTTP", &status.to_string());

    let mut buf = [0u8; 1024];
    let bytes_read = io::try_read_full(&mut response, &mut buf)
        .map_err(|e| e.0)
        .unwrap();
    log::info!(
        "Body ({} bytes):\n{}",
        bytes_read,
        std::str::from_utf8(&buf[..bytes_read]).unwrap_or("<invalid utf8>")
    );

    lcd_status(&mut lcd, "OK", &ip_str);
    log::info!("BOOT_OK");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
