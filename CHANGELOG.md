# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1] - 2026-04-04

### Added
- ESP32 firmware with configurable nightly AC schedule (7 ON/OFF slots, 22:00–07:00)
- Web dashboard with glassmorphic design served directly from the ESP32
- Manual control modes: Max Cool (16 °C) and Comfort (25 °C)
- Countdown timer with presets (30 m, 1 h, 2 h, 4 h) and custom hh:mm input
- Vacation mode: manual toggle and date-range based pause, persisted across reboots
- Optional BME280 sensor support for live room temperature and humidity
- Auto dark mode based on Singapore sunrise/sunset times
- OTA firmware updates over Wi-Fi
- Post-boot recovery: evaluates current time slot immediately after reboot
- WiFiManager captive portal for first-time Wi-Fi setup (no hardcoded credentials)
