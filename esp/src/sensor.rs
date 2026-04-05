use dht_embedded::{Dht22, DhtSensor, NoopInterruptControl};
use esp_idf_svc::hal::{
    delay::{Ets, FreeRtos},
    gpio::{Gpio10, PinDriver, Pull},
};

use crate::lcd::{self, Lcd};

/// A single temperature and humidity reading.
#[allow(dead_code)]
pub struct Reading {
    pub temperature: f32,
    pub humidity: f32,
}

/// Take 5 readings from the DHT22 on GPIO10, logging each one and showing the
/// latest on the LCD. Returns the last successful reading, or None if all fail.
pub fn read(gpio10: Gpio10, lcd: &mut Lcd<'_>) -> Option<Reading> {
    lcd::status(lcd, "DHT22", "Reading...");

    // Open-drain mode: the pin can be driven low by us or the sensor; the
    // external 4.7 kΩ pull-up brings it high when neither is pulling it down.
    let mut pin = PinDriver::input_output_od(gpio10, Pull::Floating).unwrap();
    pin.set_high().unwrap();

    // Sanity check: with the external pull-up the idle line must read high.
    // If it reads low the pull-up resistor is missing or wired incorrectly.
    FreeRtos::delay_ms(10);
    if !pin.is_high() {
        log::error!(
            "DHT22 GPIO10 reads LOW at idle — check pull-up resistor wiring"
        );
    }

    // Ets uses a busy-wait spin loop accurate to 1 µs — required for DHT22's
    // tight timing. FreeRtos has only 1 ms tick resolution and corrupts the protocol.
    let mut sensor = Dht22::new(NoopInterruptControl, Ets, pin);

    // DHT22 needs ~1 s after power-on before the first read is valid.
    FreeRtos::delay_ms(1500);

    let mut last = None;
    for i in 1..=5 {
        match sensor.read() {
            Ok(r) => {
                log::info!(
                    "DHT22 [{i}/5]: {:.1} °C  {:.1} %RH",
                    r.temperature(),
                    r.humidity()
                );
                lcd::status(
                    lcd,
                    "Temp / Humidity",
                    &format!("{:.1}C  {:.1}%", r.temperature(), r.humidity()),
                );
                last = Some(Reading {
                    temperature: r.temperature(),
                    humidity: r.humidity(),
                });
            }
            Err(e) => {
                log::warn!("DHT22 [{i}/5] error: {:?}", e);
                lcd::status(lcd, "DHT22 error", &format!("{:?}", e));
            }
        }
        // DHT22 requires at least 2 s between reads.
        if i < 5 {
            FreeRtos::delay_ms(2500);
        }
    }

    last
}
