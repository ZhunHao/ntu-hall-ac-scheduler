# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - 2026-09-19

### Added
- **Complete Rust Rewrite**: Re-architected firmware in idiomatic Rust targeting ESP32 (ESP-IDF) and RISC-V.
- **Native Host Simulator**: Added `host-sim` binary allowing local testing of the web dashboard and scheduler on macOS/Linux (`cargo run --bin host-sim`).
- **Pure-Rust Daikin IR Protocol**: Bit-level 280-bit (35-byte) Daikin protocol implementation with section checksums and 38 kHz pulse generator.
- **Hardware Task Watchdog (WDT)**: 10-second hardware watchdog timer on ESP32 to prevent lockups.
- **Physical BME280 I2C Driver**: Auto-detecting 0x76/0x77 I2C sensor driver with full Bosch compensation math.
- **Decoupled Asset Management**: Extracted frontend to `assets/index.html` compiled into binary via `include_str!`.
- **Config File Support**: `cfg.toml` support for Wi-Fi credentials, Static IP, and mDNS hostname (`ac-scheduler.local`).
- **Comprehensive Test Suite**: Unit and integration tests covering protocol encoding, slot boundaries, boot recovery, and REST API error handling.
- **CI/CD Automation**: GitHub Actions workflow for automated clippy, formatting, and unit testing.

### Removed
- Removed legacy Arduino `.ino` files and dependencies (`ac_scheduler.ino`, `ac_scheduler/`).

---

## [0.1.0] - 2026-04-04

### Added
- ESP32 firmware with configurable nightly AC schedule (7 ON/OFF slots, 22:00–07:00)
- Web dashboard with glassmorphic design served directly from the ESP32
- Manual control modes: Max Cool (16 °C) and Comfort (25 °C)
- Countdown timer with presets (30 m, 1 h, 2 h, 4 h) and custom hh:mm input
- Vacation mode: manual toggle and date-range based pause, persisted across reboots
- Optional BME280 sensor support for live room temperature and humidity
- Auto dark mode based on Singapore sunrise/sunset times
- Post-boot recovery: evaluates current time slot immediately after reboot
- WiFiManager captive portal for first-time Wi-Fi setup
