# Toro — ESP32 Firmware

Rust firmware for the Toro weather station, running on an ESP32-C3 with `esp-idf` and FreeRTOS.

## Prerequisites

```bash
sudo apt install libclang-dev libudev-dev pkg-config python3.13-venv
```

```bash
cargo install ldproxy
cargo install cargo-espflash --version 3.3.0
```

> `cargo-espflash` 4.x has a known compilation bug; use 3.3.0.

You also need to be in the `dialout` group to access the serial port:

```bash
sudo usermod -aG dialout $USER
```

Log out and back in for the group change to take effect.

## Configuration

Before building or flashing, copy the example config and fill in your Wi-Fi credentials:

```bash
cp cfg.toml.example cfg.toml
```

Then edit `cfg.toml` with your values:

```toml
[toro]
wifi_ssid = "your_ssid_here"
wifi_password = "your_password_here"
```

`cfg.toml` is gitignored and must be present for the build to succeed. The values are read by
`build.rs` and injected as compile-time env vars (`CFG_TORO_WIFI_SSID`, `CFG_TORO_WIFI_PASSWORD`).

## Build

```bash
cd esp
cargo build
```

## Flash

The firmware uses a custom partition table (`partitions.csv`) to give the app partition enough
room for the binary (~1.2 MB). `espflash.toml` wires this in automatically so no extra flags are
needed.

Connect the ESP32-C3 via USB, then from the `esp/` directory:

```bash
cargo espflash flash
```

For interactive monitoring after flashing:

```bash
cargo espflash flash --monitor
```

> Do not use `--monitor` in non-interactive terminals (CI, scripts) — it fails to initialise the
> input reader. Use `run_until.sh` instead.

## Scripted flash

For automated flashing (e.g. in CI or scripts that need to detect a sentinel string in serial
output), use `run_until.sh`:

```bash
./run_until.sh "BOOT_OK"
```

Wi-Fi association adds a few seconds to boot, so use a generous timeout when the sentinel appears
after network init:

```bash
./run_until.sh "BOOT_OK" --timeout 60
```

The script flashes the device, then reads serial output until the sentinel string appears, an error
pattern matches, or the timeout is reached. See `run_until.sh --help` for all options.
