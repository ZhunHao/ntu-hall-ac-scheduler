pub mod static_assets;

use crate::ir::{DaikinCommand, IrTransmitter};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct WebHandler {
    pub state: Arc<Mutex<AppState>>,
    pub transmitter: Arc<Mutex<dyn IrTransmitter>>,
}

pub struct WebResponse {
    pub status_code: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl WebResponse {
    pub fn ok_html(body: String) -> Self {
        Self {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            body,
        }
    }

    pub fn ok_json(body: String) -> Self {
        Self {
            status_code: 200,
            content_type: "application/json",
            body,
        }
    }

    pub fn ok_plain(body: &'static str) -> Self {
        Self {
            status_code: 200,
            content_type: "text/plain",
            body: body.to_string(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status_code: 404,
            content_type: "text/plain",
            body: "Not Found".to_string(),
        }
    }
}

impl WebHandler {
    pub fn new(state: Arc<Mutex<AppState>>, transmitter: Arc<Mutex<dyn IrTransmitter>>) -> Self {
        Self { state, transmitter }
    }

    /// Handles an incoming GET request path and query parameters.
    pub fn handle_get(&self, path: &str, query: &HashMap<String, String>) -> WebResponse {
        match path {
            "/" => WebResponse::ok_html(static_assets::INDEX_HTML.to_string()),

            "/status" => {
                let state = self.state.lock().unwrap();
                let resp = state.to_status_response();
                match serde_json::to_string(&resp) {
                    Ok(json) => WebResponse::ok_json(json),
                    Err(e) => {
                        log::error!("JSON serialization error: {}", e);
                        WebResponse {
                            status_code: 500,
                            content_type: "text/plain",
                            body: "JSON error".to_string(),
                        }
                    }
                }
            }

            "/cmd" => {
                let mode = query.get("mode").map(|s| s.as_str()).unwrap_or("");
                match mode {
                    "off" => {
                        {
                            let mut state = self.state.lock().unwrap();
                            state.manual_override = false;
                            state.set_off();
                        }
                        if let Ok(mut tx) = self.transmitter.lock() {
                            let _ = tx.send_command(&DaikinCommand::off());
                        }
                        WebResponse::ok_plain("OK")
                    }
                    "on16" => {
                        {
                            let mut state = self.state.lock().unwrap();
                            if state.is_vacation_active() {
                                state.vacation.clear();
                            }
                            state.manual_override = true;
                            state.set_on(16);
                        }
                        if let Ok(mut tx) = self.transmitter.lock() {
                            let _ = tx.send_command(&DaikinCommand::cool_at(16));
                        }
                        WebResponse::ok_plain("OK")
                    }
                    "on25" => {
                        {
                            let mut state = self.state.lock().unwrap();
                            if state.is_vacation_active() {
                                state.vacation.clear();
                            }
                            state.manual_override = true;
                            state.set_on(25);
                        }
                        if let Ok(mut tx) = self.transmitter.lock() {
                            let _ = tx.send_command(&DaikinCommand::cool_at(25));
                        }
                        WebResponse::ok_plain("OK")
                    }
                    _ => WebResponse::not_found(),
                }
            }

            "/timer" => {
                let mins: u32 = query
                    .get("min")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(1440);

                if mins > 0 {
                    {
                        let mut state = self.state.lock().unwrap();
                        if state.is_vacation_active() {
                            state.vacation.clear();
                        }
                        state.start_timer(mins);
                    }
                    if let Ok(mut tx) = self.transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::cool_at(16));
                    }
                } else {
                    {
                        let mut state = self.state.lock().unwrap();
                        state.manual_override = false;
                        state.cancel_timer();
                        state.set_off();
                    }
                    if let Ok(mut tx) = self.transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::off());
                    }
                }
                WebResponse::ok_plain("OK")
            }

            "/vacation_toggle" => {
                let send_off = {
                    let mut state = self.state.lock().unwrap();
                    if state.is_vacation_active() {
                        state.vacation.clear();
                        if state.system_status == "Vacation Mode" {
                            state.system_status = "Standby".to_string();
                        }
                        false
                    } else {
                        state.vacation.manual_vacation = true;
                        state.set_off();
                        true
                    }
                };

                if send_off {
                    if let Ok(mut tx) = self.transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::off());
                    }
                }
                WebResponse::ok_plain("OK")
            }

            "/schedule" => {
                let s_str = query.get("s").map(|s| s.as_str()).unwrap_or("0");
                let e_str = query.get("e").map(|s| s.as_str()).unwrap_or("0");
                let start: i64 = s_str.parse().unwrap_or(0);
                let end: i64 = e_str.parse().unwrap_or(0);

                let send_off = {
                    let mut state = self.state.lock().unwrap();
                    state.vacation.start_date = start;
                    state.vacation.end_date = end;

                    if state.is_vacation_active() {
                        state.set_off();
                        true
                    } else {
                        if state.system_status == "Vacation Mode" {
                            state.system_status = "Standby".to_string();
                        }
                        false
                    }
                };

                if send_off {
                    if let Ok(mut tx) = self.transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::off());
                    }
                }
                WebResponse::ok_plain("OK")
            }

            "/reset-wifi" => {
                WebResponse::ok_plain("WiFi credentials cleared. Rebooting into config portal...")
            }

            _ => WebResponse::not_found(),
        }
    }
}

pub fn parse_query_string(query_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("").to_string();
        let val = parts.next().unwrap_or("").to_string();
        map.insert(key, val);
    }
    map
}
