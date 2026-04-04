# AC Scheduler v0.1

An ESP32-based smart controller for **Daikin air conditioners**. Sends IR commands on a configurable nightly schedule, with a clean web dashboard for manual control, timers, and vacation mode.

---

## Features

- **Asymmetric schedule** — 7 configurable ON/OFF slots per night optimised for Singapore's nocturnal temperature curve (195 min total, 22:00–07:00)
- **Manual control** — Max Cool (16 °C) or Comfort (25 °C) at any time
- **Countdown timer** — presets (30 m / 1 h / 2 h / 4 h) or custom hh:mm
- **Vacation mode** — pause the schedule manually or by calendar date range; persists across reboots
- **BME280 support** — live room temperature & humidity displayed in the dashboard (optional, I²C)
- **OTA updates** — flash new firmware over Wi-Fi without a USB cable
- **Auto dark mode** — dashboard switches theme at Singapore sunrise/sunset times
- **WiFiManager** — no hardcoded credentials; configure Wi-Fi from a captive portal on first boot

---

## Hardware

| Component | Notes |
|---|---|
| ESP32-C6 (Seeed XIAO) | Any ESP32 variant works with minor pin adjustments |
| IR LED | Connected to pin `D1` (GPIO5) |
| BME280 (optional) | I²C on SDA=22, SCL=23; auto-detected at 0x76 or 0x77 |

---

## Libraries

Install via Arduino Library Manager:

- `IRremoteESP8266`
- `WiFiManager` by tzapu
- `Adafruit BME280 Library`
- `Adafruit Unified Sensor`

---

## First Boot

1. Flash `ac_scheduler.ino` to your ESP32.
2. On first boot the device creates a Wi-Fi access point: **`AC-Scheduler-Setup`** (password: `acsetup01`).
3. Connect to that AP from your phone or laptop — a captive portal opens automatically.
4. Enter your home Wi-Fi credentials and save. The device reboots and connects.
5. Find its IP from your router's DHCP table and open it in a browser, or use mDNS: **`AC-Scheduler.local`**.

To re-run the portal at any time: `GET /reset-wifi`

---

## Web Dashboard

Open `http://<device-ip>/` in any browser.

| Section | Description |
|---|---|
| **Hero card** | Current AC status, set temperature, room temp/humidity (if BME280 fitted), active timer countdown |
| **Max Cool** | Turn AC on at 16 °C |
| **Comfort** | Turn AC on at 25 °C |
| **Turn Off** | Turn AC off and cancel any timer |
| **Timer** | Run AC for a fixed duration, then auto-off |
| **Schedule** | Shows the two schedule windows (night active / day standby) |
| **Vacation Mode** | Toggle pause manually or set a departure/return date range |
| **Theme** | Light / Dark / Auto (auto switches at sunrise/sunset) |

---

## API Endpoints

| Method | Endpoint | Description |
|---|---|---|
| GET | `/` | Serve dashboard |
| GET | `/cmd?mode=on16` | Turn on at 16 °C |
| GET | `/cmd?mode=on25` | Turn on at 25 °C |
| GET | `/cmd?mode=off` | Turn off |
| GET | `/timer?min=N` | Start timer for N minutes (max 1440); `min=0` cancels |
| GET | `/status` | JSON status payload |
| GET | `/vacation_toggle` | Toggle manual vacation mode |
| GET | `/schedule?s=YYYYMMDD&e=YYYYMMDD` | Set vacation date range (`s=0&e=0` to clear) |
| GET | `/reset-wifi` | Clear saved Wi-Fi credentials and reboot into portal |

### `/status` response

```json
{
  "status": "Max Cool",
  "temp": 16,
  "slot": 1,
  "activeMins": 60,
  "timerSecs": 3542,
  "bmeOk": true,
  "rTemp": 27.4,
  "rHum": 68,
  "vacation": false,
  "manVac": false,
  "vacS": 0,
  "vacE": 0,
  "ntpOk": true
}
```

---

## Customising the Schedule

The nightly ON slots are defined in the `SCHEDULE[]` table near the top of `ac_scheduler.ino`:

```cpp
const CycleSlot SCHEDULE[] = {
    {22 * 60, 22 * 60 + 45},      // 22:00 – 22:45  (45 min)
    {23 * 60 + 15, 23 * 60 + 40}, // 23:15 – 23:40  (25 min)
    { 0 * 60 + 15,  0 * 60 + 35}, // 00:15 – 00:35  (20 min)
    { 1 * 60 + 20,  1 * 60 + 40}, // 01:20 – 01:40  (20 min)
    { 2 * 60 + 40,  3 * 60 +  0}, // 02:40 – 03:00  (20 min)
    { 4 * 60 + 15,  4 * 60 + 40}, // 04:15 – 04:40  (25 min)
    { 5 * 60 + 20,  6 * 60 +  0}, // 05:20 – 06:00  (40 min)
};
```

Each entry is `{startMinutesSinceMidnight, endMinutesSinceMidnight}`. Add, remove, or adjust rows freely — no other code needs to change. Re-flash after editing.

The active window is **22:00 – 07:00**. Outside this window the schedule is always off and `manualOverride` is cleared automatically so the next night resumes normally.

---

## OTA Updates

After first boot, subsequent flashes can be done over Wi-Fi:

1. In Arduino IDE, go to **Tools → Port** and select the network port named `AC-Scheduler`.
2. Upload as normal — you will be prompted for the OTA password: **`admin`**.

---

## Timezone

The firmware uses **GMT+8 (Singapore Standard Time)**. To change, update this constant:

```cpp
const long gmtOffset_sec = 28800; // GMT+8 — change to your offset in seconds
```

---

## License

MIT
