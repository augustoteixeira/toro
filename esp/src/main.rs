use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::{InterruptType, PinDriver, Pull},
        peripherals::Peripherals,
    },
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

    // --- Button on GPIO1 (no debounce — raw press counter) ---
    let counter = Arc::new(AtomicU32::new(0));
    let counter_isr = counter.clone();

    let mut btn = PinDriver::input(peripherals.pins.gpio1, Pull::Up).unwrap();
    btn.set_interrupt_type(InterruptType::NegEdge).unwrap();
    unsafe {
        btn.subscribe(move || {
            counter_isr.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
    }
    btn.enable_interrupt().unwrap();

    // Spawn a thread that prints the counter every second.
    let counter_thread = counter.clone();
    std::thread::spawn(move || {
        let mut last = 0u32;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let current = counter_thread.load(Ordering::Relaxed);
            if current != last {
                log::info!("Button presses: {current}");
                last = current;
            }
        }
    });

    let mut lcd = lcd::init(
        peripherals.i2c0,
        peripherals.pins.gpio2,
        peripherals.pins.gpio3,
    );

    let reading = sensor::read(peripherals.pins.gpio10, &mut lcd)
        .unwrap_or_else(|| {
            log::warn!("DHT22: no valid reading on boot, using placeholder");
            sensor::Reading {
                temperature: 0.0,
                humidity: 0.0,
            }
        });

    let _wifi = wifi::connect(peripherals.modem, sysloop, nvs, &mut lcd);

    ntp::sync(&mut lcd);

    log::info!("BOOT_OK");

    // The button driver must stay alive for interrupts to keep firing.
    let _btn = btn;

    http::run_loop(reading, &mut lcd);
}
