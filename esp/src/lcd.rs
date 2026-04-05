use esp_idf_svc::hal::{
    delay::FreeRtos,
    gpio::{Gpio2, Gpio3},
    i2c::{I2cConfig, I2cDriver, I2C0},
    units::Hertz,
};
use i2c_character_display::{CharacterDisplayPCF8574T, LcdDisplayType};

pub type Lcd<'d> = CharacterDisplayPCF8574T<I2cDriver<'d>, FreeRtos>;

/// Initialise the 16x2 LCD on I2C0, SDA=GPIO2, SCL=GPIO3.
pub fn init(
    i2c0: I2C0<'static>,
    sda: Gpio2<'static>,
    scl: Gpio3<'static>,
) -> Lcd<'static> {
    let i2c_config = I2cConfig::new().baudrate(Hertz(100_000));
    let i2c = I2cDriver::new(i2c0, sda, scl, &i2c_config).unwrap();
    let mut lcd =
        CharacterDisplayPCF8574T::new(i2c, LcdDisplayType::Lcd16x2, FreeRtos);
    lcd.init().map_err(|_| "LCD init failed").unwrap();
    log::info!("LCD initialised");
    lcd
}

/// Write a two-line status to the LCD.
/// Row 0: short label. Row 1: value.
/// Both are space-padded to 16 chars so stale content is always overwritten.
pub fn status(lcd: &mut Lcd<'_>, label: &str, value: &str) {
    let label = format!("{:<16}", &label[..label.len().min(16)]);
    let value = format!("{:<16}", &value[..value.len().min(16)]);
    lcd.set_cursor(0, 0).map_err(|_| ()).unwrap();
    lcd.print(&label).map_err(|_| ()).unwrap();
    lcd.set_cursor(0, 1).map_err(|_| ()).unwrap();
    lcd.print(&value).map_err(|_| ()).unwrap();
}
