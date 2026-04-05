use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use esp_idf_svc::hal::{
    delay::FreeRtos,
    gpio::{Gpio4, InterruptType, PinDriver, Pull},
    task::notification::Notification,
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

/// Spawn a debouncing thread for a digital input pin.
/// Emits a confirmed `PinState` on `tx` for every debounced transition.
fn spawn_debounce_task(gpio4: Gpio4<'static>, tx: Sender<PinState>) {
    std::thread::spawn(move || {
        let mut pin = PinDriver::input(gpio4, Pull::Down).unwrap();

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
///
/// Returns an `Arc<AtomicU32>` the caller can read at any time.
/// The counter only increments — it never resets or notifies.
pub fn spawn_counter_task(
    gpio4: Gpio4<'static>,
    count_on: CountOn,
) -> Arc<AtomicU32> {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_inner = counter.clone();

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        spawn_debounce_task(gpio4, tx);

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

    let peripherals =
        esp_idf_svc::hal::peripherals::Peripherals::take().unwrap();

    let counter = spawn_counter_task(peripherals.pins.gpio4, CountOn::Rising);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        log::info!("Press count: {}", counter.load(Ordering::Relaxed));
    }
}
