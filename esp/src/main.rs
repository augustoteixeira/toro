use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::peripherals::Peripherals,
    nvs::EspDefaultNvsPartition,
};

mod http;
mod lcd;
mod ntp;
mod sensor;
mod wifi;

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

    let reading = sensor::read(peripherals.pins.gpio10, &mut lcd)
        .expect("DHT22: no valid reading on boot");

    let _wifi = wifi::connect(peripherals.modem, sysloop, nvs, &mut lcd);

    ntp::sync(&mut lcd);

    log::info!("BOOT_OK");

    http::run_loop(reading, &mut lcd);
}
