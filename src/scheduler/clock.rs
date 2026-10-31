//! Wall-clock validity gate for the scheduler.
//!
//! Without a battery-backed RTC the ESP32 clock restarts at the Unix epoch (08:00 SGT) after a
//! power cut, which reads as daytime and would silently skip the night schedule. The scheduler
//! therefore runs only once the clock is known to be valid.

use chrono::{DateTime, Datelike, Utc};

/// Any clock reading before this year cannot be real time (the firmware was written in 2026).
pub const MIN_VALID_YEAR: i32 = 2026;

/// True once SNTP has synced at least once, or the system clock already shows a plausible year.
/// The year check covers software resets (watchdog panic, OTA reboot), which keep system time.
pub fn clock_ok(ntp_ok: bool, now: DateTime<Utc>) -> bool {
    ntp_ok || now.year() >= MIN_VALID_YEAR
}
