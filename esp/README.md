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

Before building or flashing, copy the example config and fill in your values:

```bash
cp cfg.toml.example cfg.toml
```

Then edit `cfg.toml`:

```toml
[toro]
wifi_ssid = "your_ssid_here"
wifi_password = "your_password_here"
server_url = "https://your-server.example.com/"
```

`cfg.toml` is gitignored and must be present for the build to succeed. All keys under `[toro]`
are read by `build.rs` and injected as compile-time env vars (`CFG_TORO_WIFI_SSID`, etc.).

## TLS

The firmware makes HTTPS requests with verified TLS. The server must have a certificate issued by
Let's Encrypt. The ISRG Root X1 certificate (Let's Encrypt's root CA) is embedded at compile time
from `certs/isrg-root-x1.pem` and passed to mbedTLS as the sole trust anchor — no other CA is
trusted.

To update the pinned certificate (e.g. when ISRG Root X1 is rotated, which is not expected before
2035):

```bash
curl https://letsencrypt.org/certs/isrgrootx1.pem -o certs/isrg-root-x1.pem
printf '\0' >> certs/isrg-root-x1.pem   # NUL terminator required by mbedTLS
```

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
