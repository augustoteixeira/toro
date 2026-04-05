use esp_idf_svc::{
    hal::delay::FreeRtos,
    sntp::{EspSntp, SyncStatus},
};

use crate::lcd::{self, Lcd};

/// Synchronise the system clock via SNTP (pool.ntp.org).
/// Blocks until the sync is complete. After this call, `SystemTime::now()`
/// returns accurate UTC time.
pub fn sync(lcd: &mut Lcd<'_>) {
    lcd::status(lcd, "NTP", "Syncing...");

    let sntp = EspSntp::new_default().unwrap();
    while sntp.get_sync_status() != SyncStatus::Completed {
        FreeRtos::delay_ms(100);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    log::info!("NTP sync complete — Unix time: {}s", now.as_secs());
    lcd::status(lcd, "NTP", "Synced");
}
