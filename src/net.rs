//! Wi-Fi recovery policy.
//!
//! If the station connection fails at boot (for example the router is still starting after the
//! same power cut), the firmware falls back to its setup AP. Without a retry it would stay there
//! and never get the time, so the schedule would never run. The firmware retries by rebooting.

/// How long the fallback AP stays up before the firmware reboots to retry the station.
pub const AP_RETRY_AFTER_SECS: u64 = 10 * 60;

/// Whether to reboot out of fallback-AP mode now. Never while someone is connected to the AP
/// (they may be entering Wi-Fi credentials), and never during the night window while the clock
/// is valid, because the schedule is running and a reboot would re-send the current slot.
pub fn should_retry_station(
    secs_in_ap: u64,
    ap_clients: usize,
    clock_ok: bool,
    night: bool,
) -> bool {
    secs_in_ap >= AP_RETRY_AFTER_SECS && ap_clients == 0 && !(clock_ok && night)
}
