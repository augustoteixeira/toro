# ESP Agent Instructions

## Credentials

Runtime config is stored in `esp/cfg.toml` (gitignored). Copy the example and fill in your values:

```bash
cp cfg.toml.example cfg.toml
# edit cfg.toml
```

`build.rs` reads `cfg.toml`, parses the `[toro]` table, and emits each key as a
`cargo:rustc-env=CFG_TORO_<KEY>=<value>` directive. Access them in Rust with `env!("CFG_TORO_<KEY>")`.

Current keys:
- `wifi_ssid` → `CFG_TORO_WIFI_SSID`
- `wifi_password` → `CFG_TORO_WIFI_PASSWORD`
- `server_url` → `CFG_TORO_SERVER_URL`

New keys are picked up automatically — no changes to `build.rs` needed.

## Environment

- `cargo` is not on the default `PATH`; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"` or rely on the shell having it set.
- `cargo-espflash` 4.x fails to compile; use version 3.3.0: `cargo install cargo-espflash --version 3.3.0`.
- The following must be installed via `apt` before building: `libclang-dev`, `libudev-dev`, `pkg-config`, `python3.13-venv`.
- `ldproxy` must be installed via `cargo install ldproxy`.
- The user must be in the `dialout` group to flash over USB.

## Partition table

The ESP32-C3 has 4 MB flash. The default ESP-IDF single-app partition table allocates only 1 MB
for the app, which is too small for the Wi-Fi-enabled Rust binary (~1.2 MB).

`partitions.csv` defines a custom layout with a 1.875 MB factory partition:

```
nvs       0x9000   24 KB
phy_init  0xF000    4 KB
factory   0x10000  1875 KB
```

`espflash.toml` (top-level key, not under `[flash]`) points `cargo espflash` at this CSV so it is
used automatically on every flash without extra flags:

```toml
partition_table = "partitions.csv"
```

Do **not** nest `partition_table` under `[flash]` — espflash 3.3.0 ignores it there.

## Flashing

Do **not** run `cargo espflash flash --monitor` directly in scripts. Use `esp/run_until.sh
<sentinel>` instead — it runs the flash command as a background process and reads serial output
until the sentinel string appears.

```bash
./run_until.sh "BOOT_OK" --timeout 60
```

Wi-Fi association takes a few seconds, so the default 60 s timeout is appropriate when `BOOT_OK`
is logged after network init. Pass `--timeout 200` if the device also needs to do slow first-boot
work (NVS init, PHY calibration, etc.).

See `esp/run_until.sh` for full usage.

## TLS

HTTPS requests use verified TLS via mbedTLS (built into ESP-IDF). The trust anchor is the ISRG
Root X1 certificate, embedded at compile time from `certs/isrg-root-x1.pem`:

```rust
const CA_CERT: X509<'static> =
    X509::pem_until_nul(include_bytes!("../certs/isrg-root-x1.pem"));
```

The file **must** end with a NUL byte — mbedTLS requires it for PEM input. The file in the repo
already has this; if you ever refresh it:

```bash
curl https://letsencrypt.org/certs/isrgrootx1.pem -o certs/isrg-root-x1.pem
printf '\0' >> certs/isrg-root-x1.pem
```

`server_certificate: Some(CA_CERT)` in `HttpConfig` pins this CA as the sole trust anchor.
No other CA (including the built-in Mozilla bundle) is trusted for connections made with this
client. This is intentional — it means only servers with a Let's Encrypt certificate will be
accepted.

To use a self-signed CA instead: generate a CA with `openssl`, replace `certs/isrg-root-x1.pem`
with your CA cert (NUL-terminated), and update the `const` name for clarity.

## cfg.toml format

Keys must be plain TOML strings. A stray trailing quote or other syntax error will cause `build.rs`
to panic with a clear message at compile time. Keep the file minimal:

```toml
[toro]
wifi_ssid = "your_ssid"
wifi_password = "your_password"
server_url = "https://your-server.example.com/"
```
