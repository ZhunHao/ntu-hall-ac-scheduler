use ac_scheduler::ir::{IrTransmitter, MockTransmitter};
use ac_scheduler::net::{should_retry_station, AP_RETRY_AFTER_SECS};
use ac_scheduler::scheduler::{clock_ok, SchedulerAction, SchedulerEngine, SCHEDULED_SETPOINT_C};
use ac_scheduler::state::AppState;
use ac_scheduler::storage::{InMemoryStorage, TimeStorage};
use ac_scheduler::web::{parse_query_string, WebHandler};
use chrono::{TimeZone, Utc};
use std::sync::{Arc, Mutex};

#[test]
fn clock_invalid_at_epoch_until_ntp_syncs() {
    // A power cut restarts the ESP32 clock at the Unix epoch, which reads as 08:00 SGT
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    assert!(!clock_ok(false, epoch));
    assert!(clock_ok(true, epoch));
}

#[test]
fn clock_valid_after_software_reset_without_ntp() {
    // A watchdog or OTA reboot keeps system time, so a plausible year is enough
    let now = Utc.with_ymd_and_hms(2026, 10, 30, 14, 0, 0).unwrap();
    assert!(clock_ok(false, now));
    let last_year = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();
    assert!(!clock_ok(false, last_year));
}

#[test]
fn schedule_idle_while_clock_not_set() {
    let mut engine = SchedulerEngine::new();
    // Inside slot 1 by wall time, but the clock is not trusted yet
    assert_eq!(engine.step(false, 22, 10, false, false), None);
    assert_eq!(engine.step(false, 22, 11, false, false), None);
    assert!(!engine.recovered);
}

#[test]
fn first_valid_step_inside_slot_turns_on() {
    let mut engine = SchedulerEngine::new();
    // Epoch time reads 08:00 SGT: nothing to do, and recovery must not be used up
    assert_eq!(engine.step(false, 8, 0, false, false), None);
    // SNTP syncs during slot 2 (23:15-23:40)
    assert_eq!(
        engine.step(true, 23, 20, false, false),
        Some(SchedulerAction::TurnOn(SCHEDULED_SETPOINT_C))
    );
    assert!(engine.recovered);
    // No duplicate send on the next step; the slot then ends normally
    assert_eq!(engine.step(true, 23, 21, false, false), None);
    assert_eq!(
        engine.step(true, 23, 40, false, false),
        Some(SchedulerAction::TurnOff)
    );
}

#[test]
fn first_valid_step_between_slots_waits_for_next_slot() {
    let mut engine = SchedulerEngine::new();
    assert_eq!(engine.step(true, 23, 0, false, false), None);
    assert_eq!(
        engine.step(true, 23, 15, false, false),
        Some(SchedulerAction::TurnOn(SCHEDULED_SETPOINT_C))
    );
}

#[test]
fn first_valid_step_respects_manual_override() {
    // Max Cool pressed on the dashboard before the clock was set: recovery must not override it
    let mut engine = SchedulerEngine::new();
    assert_eq!(engine.step(true, 22, 10, false, true), None);
    assert!(engine.recovered);
    // The override still ends at the 07:00 boundary
    assert_eq!(
        engine.step(true, 7, 0, false, true),
        Some(SchedulerAction::TurnOffAndClearOverride)
    );
}

#[test]
fn first_valid_step_during_vacation_does_nothing() {
    let mut engine = SchedulerEngine::new();
    assert_eq!(engine.step(true, 22, 10, true, false), None);
    assert_eq!(engine.step(true, 22, 11, true, false), None);
}

#[test]
fn status_reports_clock_state() {
    let mut state = AppState::new();
    assert_eq!(state.to_status_response().clock, "not_set");
    state.clock_ok = true;
    let json = serde_json::to_value(state.to_status_response()).unwrap();
    assert_eq!(json["clock"], "ok");
}

#[test]
fn max_cool_and_legacy_on16_both_send_22() {
    for mode in ["mode=on22", "mode=on16"] {
        let state = Arc::new(Mutex::new(AppState::new()));
        let mock = Arc::new(Mutex::new(MockTransmitter::new()));
        let transmitter: Arc<Mutex<dyn IrTransmitter>> =
            Arc::clone(&mock) as Arc<Mutex<dyn IrTransmitter>>;
        let handler = WebHandler::new(Arc::clone(&state), transmitter);

        let resp = handler.handle_get("/cmd", &parse_query_string(mode));
        assert_eq!(resp.status_code, 200);
        assert_eq!(state.lock().unwrap().system_status, "Max Cool");
        assert_eq!(mock.lock().unwrap().last_command.unwrap().temp_celsius, 22);
    }
}

#[test]
fn timer_cools_at_22() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let mock = Arc::new(Mutex::new(MockTransmitter::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> =
        Arc::clone(&mock) as Arc<Mutex<dyn IrTransmitter>>;
    let handler = WebHandler::new(Arc::clone(&state), transmitter);

    handler.handle_get("/timer", &parse_query_string("min=30"));
    assert_eq!(state.lock().unwrap().current_temp, 22);
    assert_eq!(mock.lock().unwrap().last_command.unwrap().temp_celsius, 22);
}

#[test]
fn last_known_time_round_trips() {
    let mut storage = InMemoryStorage::new();
    assert_eq!(storage.load_last_known_time().unwrap(), None);
    storage.save_last_known_time(1_800_000_000).unwrap();
    assert_eq!(storage.load_last_known_time().unwrap(), Some(1_800_000_000));
}

#[test]
fn ap_retry_waits_for_timeout_and_idle_ap() {
    assert!(!should_retry_station(
        AP_RETRY_AFTER_SECS - 1,
        0,
        false,
        false
    ));
    assert!(should_retry_station(AP_RETRY_AFTER_SECS, 0, false, false));
    // Someone is on the setup AP entering credentials
    assert!(!should_retry_station(AP_RETRY_AFTER_SECS, 1, false, false));
}

#[test]
fn ap_retry_never_interrupts_a_running_night_schedule() {
    // Valid clock at night: the schedule is running, so a reboot would re-send the slot
    assert!(!should_retry_station(
        AP_RETRY_AFTER_SECS * 6,
        0,
        true,
        true
    ));
    // No valid clock at night: nothing is running, so retrying is the only way to get time
    assert!(should_retry_station(AP_RETRY_AFTER_SECS, 0, false, true));
    // Daytime with a valid clock: safe to retry
    assert!(should_retry_station(AP_RETRY_AFTER_SECS, 0, true, false));
}
