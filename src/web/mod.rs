pub mod static_assets;

use crate::ir::{DaikinCommand, IrTransmitter};
use crate::scheduler::VacationSettings;
use crate::state::AppState;
use crate::storage::{InMemoryStorage, VacationStorage, WifiCredentials, WifiStorage};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct WebHandler {
    pub state: Arc<Mutex<AppState>>,
    pub transmitter: Arc<Mutex<dyn IrTransmitter>>,
    storage: Arc<Mutex<dyn VacationStorage>>,
    wifi_storage: Option<Arc<Mutex<dyn WifiStorage>>>,
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
        let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
        Self::with_all_storage(state, transmitter, storage.clone(), Some(storage))
    }

    pub fn with_storage(
        state: Arc<Mutex<AppState>>,
        transmitter: Arc<Mutex<dyn IrTransmitter>>,
        storage: Arc<Mutex<dyn VacationStorage>>,
    ) -> Self {
        Self {
            state,
            transmitter,
            storage,
            wifi_storage: None,
        }
    }

    pub fn with_all_storage(
        state: Arc<Mutex<AppState>>,
        transmitter: Arc<Mutex<dyn IrTransmitter>>,
        storage: Arc<Mutex<dyn VacationStorage>>,
        wifi_storage: Option<Arc<Mutex<dyn WifiStorage>>>,
    ) -> Self {
        Self {
            state,
            transmitter,
            storage,
            wifi_storage,
        }
    }

    // The caller holds the state lock so concurrent changes cannot overtake this save.
    // Do not apply the command or send IR if persistence fails.
    fn save_vacation(
        &self,
        state: &mut AppState,
        vacation: VacationSettings,
    ) -> Result<(), WebResponse> {
        self.storage.lock().unwrap().save(&vacation).map_err(|e| {
            log::error!("Failed to save vacation settings: {}", e);
            WebResponse {
                status_code: 500,
                content_type: "text/plain",
                body: "Failed to save vacation settings".to_string(),
            }
        })?;
        state.vacation = vacation;
        Ok(())
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
                                if let Err(response) =
                                    self.save_vacation(&mut state, VacationSettings::default())
                                {
                                    return response;
                                }
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
                                if let Err(response) =
                                    self.save_vacation(&mut state, VacationSettings::default())
                                {
                                    return response;
                                }
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
                            if let Err(response) =
                                self.save_vacation(&mut state, VacationSettings::default())
                            {
                                return response;
                            }
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
                        if let Err(response) =
                            self.save_vacation(&mut state, VacationSettings::default())
                        {
                            return response;
                        }
                        if state.system_status == "Vacation Mode" {
                            state.system_status = "Standby".to_string();
                        }
                        false
                    } else {
                        let mut vacation = state.vacation;
                        vacation.manual_vacation = true;
                        if let Err(response) = self.save_vacation(&mut state, vacation) {
                            return response;
                        }
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
                    let mut vacation = state.vacation;
                    vacation.start_date = start;
                    vacation.end_date = end;
                    if let Err(response) = self.save_vacation(&mut state, vacation) {
                        return response;
                    }

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
                if let Some(ref ws) = self.wifi_storage {
                    match ws.lock().unwrap().clear_wifi() {
                        Ok(()) => {
                            #[cfg(target_os = "espidf")]
                            {
                                std::thread::spawn(|| {
                                    std::thread::sleep(std::time::Duration::from_millis(1500));
                                    unsafe {
                                        esp_idf_sys::esp_restart();
                                    }
                                });
                            }
                            WebResponse::ok_plain(
                                "WiFi credentials cleared. Rebooting into config portal...",
                            )
                        }
                        Err(e) => WebResponse {
                            status_code: 500,
                            content_type: "text/plain",
                            body: format!("Failed to clear Wi-Fi credentials: {e}"),
                        },
                    }
                } else {
                    WebResponse::ok_plain(
                        "WiFi credentials cleared. Rebooting into config portal...",
                    )
                }
            }

            "/wifi" | "/hotspot-detect.html" | "/generate_204" | "/ncsi.txt" => {
                WebResponse::ok_html(static_assets::WIFI_SETUP_HTML.to_string())
            }

            "/wifi_save" => {
                let s = query.get("s").map(|s| s.as_str()).unwrap_or("");
                let p = query.get("p").map(|s| s.as_str()).unwrap_or("");
                if s.is_empty() {
                    return WebResponse {
                        status_code: 400,
                        content_type: "text/plain",
                        body: "SSID cannot be empty".to_string(),
                    };
                }
                if let Some(ref ws) = self.wifi_storage {
                    let creds = WifiCredentials {
                        ssid: s.to_string(),
                        password: p.to_string(),
                    };
                    match ws.lock().unwrap().save_wifi(&creds) {
                        Ok(()) => {
                            #[cfg(target_os = "espidf")]
                            {
                                std::thread::spawn(|| {
                                    std::thread::sleep(std::time::Duration::from_millis(1500));
                                    unsafe {
                                        esp_idf_sys::esp_restart();
                                    }
                                });
                            }
                            WebResponse::ok_plain("WiFi credentials saved. Rebooting to connect...")
                        }
                        Err(e) => WebResponse {
                            status_code: 500,
                            content_type: "text/plain",
                            body: format!("Failed to save Wi-Fi credentials: {e}"),
                        },
                    }
                } else {
                    WebResponse::ok_plain("WiFi credentials saved. Rebooting to connect...")
                }
            }

            "/update" => WebResponse::ok_html(static_assets::OTA_UPDATE_HTML.to_string()),

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
