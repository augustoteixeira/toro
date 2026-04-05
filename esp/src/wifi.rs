use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::delay::FreeRtos,
    nvs::EspDefaultNvsPartition,
    wifi::{
        AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
    },
};

use crate::lcd::{self, Lcd};

const SSID: &str = env!("CFG_TORO_WIFI_SSID");
const PASSWORD: &str = env!("CFG_TORO_WIFI_PASSWORD");

/// Connect to Wi-Fi and wait for an IP address.
/// Updates the LCD at each stage. Returns the connected `BlockingWifi` driver
/// so the caller can keep it alive (dropping it disconnects).
pub fn connect<'d>(
    modem: impl esp_idf_svc::hal::modem::WifiModemPeripheral + 'd,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    lcd: &mut Lcd<'_>,
) -> BlockingWifi<EspWifi<'d>> {
    lcd::status(lcd, "Wi-Fi", "Connecting...");

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs)).unwrap(),
        sysloop,
    )
    .unwrap();

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        password: PASSWORD.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))
    .unwrap();

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
                lcd::status(lcd, "Wi-Fi", &format!("Retry {attempt}/5..."));
                FreeRtos::delay_ms(2000);
            }
        }
    }
    if !connected {
        lcd::status(lcd, "Wi-Fi FAILED", "Check credentials");
        panic!("Wi-Fi: all connect attempts failed");
    }

    lcd::status(lcd, "Wi-Fi", "Getting IP...");
    wifi.wait_netif_up().unwrap();

    let ip = wifi.wifi().sta_netif().get_ip_info().unwrap();
    let ip_str = ip.ip.to_string();
    log::info!("IP address: {}", ip_str);
    lcd::status(lcd, "IP:", &ip_str);

    wifi
}
