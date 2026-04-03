use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::utils::io;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    http::client::{Configuration as HttpConfig, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    wifi::{
        AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
    },
};

const WIFI_SSID: &str = env!("CFG_TORO_WIFI_SSID");
const WIFI_PASSWORD: &str = env!("CFG_TORO_WIFI_PASSWORD");

fn main() {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

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

    // HTTP GET example.com
    let mut client = HttpClient::wrap(
        EspHttpConnection::new(&HttpConfig::default()).unwrap(),
    );
    let request = client.get("http://example.com").unwrap();
    log::info!("-> GET http://example.com");
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

    // Keep the task (and wifi) alive so the IP lease is not dropped
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
