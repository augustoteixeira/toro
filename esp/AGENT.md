# ESP Agent Instructions

## Environment

- `cargo` is not on the default `PATH`; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"` or rely on the shell having it set.
- `cargo-espflash` 4.x fails to compile; use version 3.3.0.
- The following must be installed via `apt` before building: `libclang-dev`, `libudev-dev`, `pkg-config`, `python3.13-venv`.
- `ldproxy` must be installed via `cargo install ldproxy`.
- The user must be in the `dialout` group to flash over USB.

## Flashing

Do **not** run `cargo espflash flash --monitor` directly. Use `esp/run_until.sh <sentinel>` instead — it runs the flash command as a background process and exits cleanly once the sentinel string appears in the serial output.

```bash
./run_until.sh "BOOT_OK"
```

See `esp/run_until.sh` for full usage.
