use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        delay::FreeRtos,
        gpio::{InputPin, InterruptType, PinDriver, Pull},
        peripherals::Peripherals,
        task::notification::Notification,
    },
    nvs::EspDefaultNvsPartition,
};

mod http;
mod lcd;
mod ntp;
mod sensor;
mod wifi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinState {
    High,
    Low,
}

/// Which transition the counter thread should count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountOn {
    Rising,  // count PinState::High (Low → High)
    Falling, // count PinState::Low  (High → Low)
}

/// Spawn a debouncing thread for any digital input pin.
/// Emits a confirmed `PinState` on `tx` for every debounced transition.
fn spawn_debounce_task<P>(pin: P, tx: Sender<PinState>)
where
    P: InputPin + 'static,
{
    std::thread::spawn(move || {
        let mut pin = PinDriver::input(pin, Pull::Down).unwrap();

        let mut state = if pin.is_high() {
            PinState::High
        } else {
            PinState::Low
        };
        log::info!("Debounce task started — initial state: {:?}", state);

        let notification = Notification::new();
        let notifier = notification.notifier();

        unsafe {
            pin.subscribe(move || {
                unsafe {
                    notifier.notify_and_yield(NonZeroU32::new(1).unwrap())
                };
            })
            .unwrap();
        }

        let next_edge = |s: PinState| match s {
            PinState::Low => InterruptType::PosEdge,
            PinState::High => InterruptType::NegEdge,
        };
        pin.set_interrupt_type(next_edge(state)).unwrap();
        pin.enable_interrupt().unwrap();

        loop {
            notification.wait_any();
            FreeRtos::delay_ms(25);
            let confirmed = if pin.is_high() {
                PinState::High
            } else {
                PinState::Low
            };

            if confirmed != state {
                state = confirmed;
                let _ = tx.send(state);
            } else {
                log::debug!("Spurious edge ignored, staying {:?}", state);
            }

            pin.set_interrupt_type(next_edge(state)).unwrap();
            pin.enable_interrupt().unwrap();
        }
    });
}

/// Spawn a counter thread that owns a debouncer and counts confirmed transitions.
/// Returns an `Arc<AtomicU32>` the caller can read at any time.
pub fn spawn_counter_task<P>(pin: P, count_on: CountOn) -> Arc<AtomicU32>
where
    P: InputPin + 'static,
{
    let counter = Arc::new(AtomicU32::new(0));
    let counter_inner = counter.clone();

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        spawn_debounce_task(pin, tx);

        loop {
            let state = rx.recv().unwrap();
            let should_count = match count_on {
                CountOn::Rising => state == PinState::High,
                CountOn::Falling => state == PinState::Low,
            };
            if should_count {
                counter_inner.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    counter
}

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    // Button counter on GPIO4 — started first so it is ready immediately.
    let press_count =
        spawn_counter_task(peripherals.pins.gpio4, CountOn::Rising);

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

    let boot_unix = ntp::fetch(&mut lcd);

    log::info!("BOOT_OK");

    http::run_loop(reading, boot_unix, &mut lcd);
}
