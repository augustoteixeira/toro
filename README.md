# Toro

A DIY weather station built with Rust.

- **Firmware**: runs on an ESP32 using `esp-idf` with FreeRTOS
- **Server**: Rocket-based web server for the frontend

## Building the firmware (esp)

### Prerequisites

```
sudo apt install libclang-dev
cargo install ldproxy
```

### Build

```
cd esp
cargo build
```

## License

MIT — see [LICENSE](LICENSE)
