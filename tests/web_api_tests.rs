use ac_scheduler::ir::{IrTransmitter, MockTransmitter};
use ac_scheduler::scheduler::VacationSettings;
use ac_scheduler::state::AppState;
use ac_scheduler::storage::{InMemoryStorage, VacationStorage};
use ac_scheduler::web::{parse_query_string, WebHandler};
use std::sync::{Arc, Mutex};

#[test]
fn vacation_changes_survive_reloading_storage() {
    let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
    let state = Arc::new(Mutex::new(AppState::new()));
    let handler = WebHandler::with_storage(
        Arc::clone(&state),
        Arc::new(Mutex::new(MockTransmitter::new())),
        storage.clone(),
    );
    for (path, query, expected) in [
        (
            "/schedule",
            "s=20990101&e=20990110",
            VacationSettings::new(false, 20990101, 20990110),
        ),
        (
            "/vacation_toggle",
            "",
            VacationSettings::new(true, 20990101, 20990110),
        ),
        ("/vacation_toggle", "", VacationSettings::default()),
        (
            "/schedule",
            "s=20990201&e=20990210",
            VacationSettings::new(false, 20990201, 20990210),
        ),
        ("/schedule", "s=0&e=0", VacationSettings::default()),
    ] {
        assert_eq!(
            handler
                .handle_get(path, &parse_query_string(query))
                .status_code,
            200
        );
        assert_eq!(storage.lock().unwrap().load().unwrap(), expected);
        assert_eq!(state.lock().unwrap().vacation, expected);
    }
}

#[test]
fn manual_cooling_persists_vacation_cancellation() {
    for (path, query) in [
        ("/cmd", "mode=on16"),
        ("/cmd", "mode=on25"),
        ("/timer", "min=30"),
    ] {
        let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
        let state = Arc::new(Mutex::new(AppState::new()));
        let handler = WebHandler::with_storage(
            state,
            Arc::new(Mutex::new(MockTransmitter::new())),
            storage.clone(),
        );
        handler.handle_get("/vacation_toggle", &parse_query_string(""));
        assert!(storage.lock().unwrap().load().unwrap().manual_vacation);
        assert_eq!(
            handler
                .handle_get(path, &parse_query_string(query))
                .status_code,
            200
        );
        assert_eq!(
            storage.lock().unwrap().load().unwrap(),
            VacationSettings::default()
        );
    }
}

struct FailingStorage;
impl VacationStorage for FailingStorage {
    fn load(&self) -> Result<VacationSettings, String> {
        Ok(VacationSettings::default())
    }
    fn save(&mut self, _: &VacationSettings) -> Result<(), String> {
        Err("flash write failed".into())
    }
}

#[test]
fn reset_wifi_reports_that_compiled_credentials_cannot_be_reset() {
    let handler = WebHandler::new(
        Arc::new(Mutex::new(AppState::new())),
        Arc::new(Mutex::new(MockTransmitter::new())),
    );
    let response = handler.handle_get("/reset-wifi", &parse_query_string(""));
    assert_eq!(response.status_code, 501);
    assert!(response.body.contains("cfg.toml"));
}

#[test]
fn failed_vacation_save_leaves_state_and_ir_unchanged() {
    for (path, query, vacation) in [
        ("/vacation_toggle", "", VacationSettings::default()),
        ("/vacation_toggle", "", VacationSettings::new(true, 0, 0)),
        (
            "/schedule",
            "s=20990101&e=20990110",
            VacationSettings::default(),
        ),
        ("/cmd", "mode=on16", VacationSettings::new(true, 0, 0)),
        ("/cmd", "mode=on25", VacationSettings::new(true, 0, 0)),
        ("/timer", "min=30", VacationSettings::new(true, 0, 0)),
    ] {
        let mut initial = AppState::new();
        initial.vacation = vacation;
        let state = Arc::new(Mutex::new(initial));
        let tx = Arc::new(Mutex::new(MockTransmitter::new()));
        let handler = WebHandler::with_storage(
            state.clone(),
            tx.clone(),
            Arc::new(Mutex::new(FailingStorage)),
        );
        assert_eq!(
            handler
                .handle_get(path, &parse_query_string(query))
                .status_code,
            500
        );
        let state = state.lock().unwrap();
        assert_eq!(state.vacation, vacation);
        assert_eq!(state.system_status, "Standby");
        assert!(!state.manual_override);
        assert!(state.timer.is_none());
        assert_eq!(tx.lock().unwrap().send_count, 0);
    }
}

#[test]
fn test_status_response_schema() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let handler = WebHandler::new(state, transmitter);

    let query = parse_query_string("");
    let resp = handler.handle_get("/status", &query);
    assert_eq!(resp.status_code, 200);
    assert_eq!(resp.content_type, "application/json");

    let val: serde_json::Value = serde_json::from_str(&resp.body).expect("Valid JSON");
    assert_eq!(val["status"], "Standby");
    assert_eq!(val["temp"], 24);
    assert_eq!(val["slot"], 2);
    assert_eq!(val["bmeOk"], false);
    assert_eq!(val["vacation"], false);
    assert_eq!(val["manVac"], false);
    assert_eq!(val["vacS"], 0);
    assert_eq!(val["vacE"], 0);
}

#[test]
fn test_cmd_on16_and_off() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let mock = Arc::new(Mutex::new(MockTransmitter::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> =
        Arc::clone(&mock) as Arc<Mutex<dyn IrTransmitter>>;
    let handler = WebHandler::new(Arc::clone(&state), Arc::clone(&transmitter));

    // Turn on 16
    let query = parse_query_string("mode=on16");
    let resp = handler.handle_get("/cmd", &query);
    assert_eq!(resp.status_code, 200);
    assert_eq!(resp.body, "OK");

    {
        let s = state.lock().unwrap();
        assert_eq!(s.system_status, "Max Cool");
        assert_eq!(s.current_temp, 16);
        assert!(s.manual_override);
    }
    {
        let tx = mock.lock().unwrap();
        assert_eq!(tx.send_count, 1);
        let cmd = tx.last_command.unwrap();
        assert!(cmd.power);
        assert_eq!(cmd.temp_celsius, 16);
    }

    // Turn off
    let query = parse_query_string("mode=off");
    let resp = handler.handle_get("/cmd", &query);
    assert_eq!(resp.status_code, 200);

    {
        let s = state.lock().unwrap();
        assert_eq!(s.system_status, "Standby");
        assert!(!s.manual_override);
    }
    {
        let tx = mock.lock().unwrap();
        assert_eq!(tx.send_count, 2);
        let cmd = tx.last_command.unwrap();
        assert!(!cmd.power);
    }
}

#[test]
fn test_timer_countdown_endpoint() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let mock = Arc::new(Mutex::new(MockTransmitter::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> =
        Arc::clone(&mock) as Arc<Mutex<dyn IrTransmitter>>;
    let handler = WebHandler::new(Arc::clone(&state), Arc::clone(&transmitter));

    // Start 60-minute timer
    let query = parse_query_string("min=60");
    let resp = handler.handle_get("/timer", &query);
    assert_eq!(resp.status_code, 200);

    {
        let s = state.lock().unwrap();
        assert!(s.timer.is_some());
        assert_eq!(s.system_status, "Max Cool");
        assert_eq!(s.current_temp, 16);
        let status_resp = s.to_status_response();
        assert_eq!(status_resp.active_mins, 60);
        assert!(status_resp.timer_secs > 3590);
    }

    // Cancel timer
    let query = parse_query_string("min=0");
    let resp = handler.handle_get("/timer", &query);
    assert_eq!(resp.status_code, 200);

    {
        let s = state.lock().unwrap();
        assert!(s.timer.is_none());
        assert_eq!(s.system_status, "Standby");
        let status_resp = s.to_status_response();
        assert_eq!(status_resp.active_mins, 0);
        assert_eq!(status_resp.timer_secs, 0);
    }
}

#[test]
fn test_vacation_endpoints() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let mock = Arc::new(Mutex::new(MockTransmitter::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> =
        Arc::clone(&mock) as Arc<Mutex<dyn IrTransmitter>>;
    let handler = WebHandler::new(Arc::clone(&state), Arc::clone(&transmitter));

    // Toggle Vacation ON
    let query = parse_query_string("");
    let resp = handler.handle_get("/vacation_toggle", &query);
    assert_eq!(resp.status_code, 200);

    {
        let s = state.lock().unwrap();
        assert!(s.vacation.manual_vacation);
        assert_eq!(s.system_status, "Vacation Mode");
    }

    // Turning on AC auto-cancels vacation mode
    let query = parse_query_string("mode=on25");
    let resp = handler.handle_get("/cmd", &query);
    assert_eq!(resp.status_code, 200);

    {
        let s = state.lock().unwrap();
        assert!(!s.vacation.manual_vacation);
        assert_eq!(s.system_status, "Comfort");
        assert_eq!(s.current_temp, 25);
    }
}

#[test]
fn test_unknown_route_returns_404() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let handler = WebHandler::new(state, transmitter);

    let query = parse_query_string("");
    let resp = handler.handle_get("/nonexistent", &query);
    assert_eq!(resp.status_code, 404);
}

#[test]
fn test_invalid_cmd_returns_404() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let handler = WebHandler::new(state, transmitter);

    let query = parse_query_string("mode=turbo_boost");
    let resp = handler.handle_get("/cmd", &query);
    assert_eq!(resp.status_code, 404);
}

#[test]
fn test_timer_edge_cases() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let handler = WebHandler::new(Arc::clone(&state), transmitter);

    // Excessively large minutes clamped to 1440
    let query = parse_query_string("min=50000");
    let resp = handler.handle_get("/timer", &query);
    assert_eq!(resp.status_code, 200);
    {
        let s = state.lock().unwrap();
        assert_eq!(s.timer.as_ref().unwrap().initial_minutes, 1440);
    }

    // Invalid non-numeric input defaults to 0 (cancels timer, turns off)
    let query = parse_query_string("min=abc");
    let resp = handler.handle_get("/timer", &query);
    assert_eq!(resp.status_code, 200);
    {
        let s = state.lock().unwrap();
        assert!(s.timer.is_none());
        assert_eq!(s.system_status, "Standby");
    }
}

#[test]
fn test_schedule_invalid_query_fallback() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let handler = WebHandler::new(Arc::clone(&state), transmitter);

    let query = parse_query_string("s=not_a_date&e=invalid");
    let resp = handler.handle_get("/schedule", &query);
    assert_eq!(resp.status_code, 200);
    {
        let s = state.lock().unwrap();
        assert_eq!(s.vacation.start_date, 0);
        assert_eq!(s.vacation.end_date, 0);
    }
}
