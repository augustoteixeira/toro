# Toro

A DIY weather station built with Rust.

- **Firmware**: runs on an ESP32 using `esp-idf` with FreeRTOS
- **Server**: Rocket-based web server for the frontend

## Building the firmware (esp)

### Prerequisites

```
sudo apt install libclang-dev libudev-dev
cargo install ldproxy
cargo install cargo-espflash --version 3.3.0
```

> `cargo-espflash` 4.x has a known compilation bug; use 3.3.0.

You also need to be in the `dialout` group to access the serial port:

```
sudo usermod -aG dialout $USER
```

Log out and back in for the group change to take effect.

### Build

```
cd esp
cargo build
```

### Flash and monitor

Connect the ESP32-C3 via USB, then from the `esp` directory:

```
cargo espflash flash --monitor
```

## License

MIT — see [LICENSE](LICENSE)
