fn main() {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    esp_idf_svc::hal::task::block_on(async {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        log::info!("Hello, world!");
        log::info!("BOOT_OK");
    });
}
