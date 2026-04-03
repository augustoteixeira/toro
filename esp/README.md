# Toro — ESP32 Firmware

Rust firmware for the Toro weather station, running on an ESP32-C3 with `esp-idf` and FreeRTOS.

## Prerequisites

```bash
sudo apt install libclang-dev libudev-dev pkg-config
cargo install ldproxy
cargo install cargo-espflash --version 3.3.0
```

> `cargo-espflash` 4.x has a known compilation bug; use 3.3.0.

You also need to be in the `dialout` group to access the serial port:

```bash
sudo usermod -aG dialout $USER
```

Log out and back in for the group change to take effect.

## Build

```bash
cd esp
cargo build
```

## Flash and monitor

Connect the ESP32-C3 via USB, then from the `esp/` directory:

```bash
cargo espflash flash --monitor
```

## Scripted flash

For automated flashing (e.g. in CI or scripts that need to detect a sentinel string in serial output), use `run_until.sh`:

```bash
./run_until.sh "BOOT_OK"
```

The script runs the flash command as a background process and exits cleanly once the sentinel string appears in the serial output.
