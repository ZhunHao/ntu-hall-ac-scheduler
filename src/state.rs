//! Shared application state and JSON status model.

use crate::scheduler::VacationSettings;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ActiveTimer {
    pub start_instant: Instant,
    pub duration_secs: u64,
    pub initial_minutes: u32,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub system_status: String,
    pub current_temp: u8,
    pub current_slot: u32, // 1 for night active, 2 for day standby
    pub manual_override: bool,

    // Timer
    pub timer: Option<ActiveTimer>,

    // Vacation settings
    pub vacation: VacationSettings,

    // Environmental sensor
    pub bme_ok: bool,
    pub room_temp: f32,
    pub room_humidity: f32,

    // NTP status
    pub ntp_ok: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            system_status: "Standby".to_string(),
            current_temp: 24,
            current_slot: 2,
            manual_override: false,
            timer: None,
            vacation: VacationSettings::default(),
            bme_ok: false,
            room_temp: 24.0,
            room_humidity: 50.0,
            ntp_ok: false,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_on(&mut self, temp: u8) {
        self.current_temp = temp;
        self.system_status = if temp < 20 {
            "Max Cool".to_string()
        } else {
            "Comfort".to_string()
        };
    }

    pub fn set_off(&mut self) {
        self.timer = None;
        if self.is_vacation_active() {
            self.system_status = "Vacation Mode".to_string();
        } else {
            self.system_status = "Standby".to_string();
        }
    }

    pub fn start_timer(&mut self, minutes: u32) {
        let secs = (minutes as u64) * 60;
        self.timer = Some(ActiveTimer {
            start_instant: Instant::now(),
            duration_secs: secs,
            initial_minutes: minutes,
        });
        self.manual_override = true;
        self.set_on(16);
    }

    pub fn cancel_timer(&mut self) {
        self.timer = None;
    }

    pub fn remaining_timer_secs(&self) -> i64 {
        if let Some(ref timer) = self.timer {
            let elapsed = timer.start_instant.elapsed().as_secs();
            if elapsed < timer.duration_secs {
                (timer.duration_secs - elapsed) as i64
            } else {
                0
            }
        } else {
            0
        }
    }

    pub fn is_vacation_active(&self) -> bool {
        // Evaluate manual vacation or if date range matches today
        let now_ymd = current_date_ymd();
        self.vacation.is_active(now_ymd)
    }

    pub fn to_status_response(&self) -> StatusResponse {
        let is_vac_active = self.is_vacation_active();
        let status = if is_vac_active && self.system_status == "Standby" {
            "Vacation Mode".to_string()
        } else {
            self.system_status.clone()
        };

        let (active_mins, timer_secs) = if let Some(ref timer) = self.timer {
            (timer.initial_minutes, self.remaining_timer_secs())
        } else {
            (0, 0)
        };

        StatusResponse {
            status,
            temp: self.current_temp,
            slot: self.current_slot,
            active_mins,
            timer_secs,
            bme_ok: self.bme_ok,
            r_temp: self.room_temp,
            r_hum: self.room_humidity,
            vacation: is_vac_active,
            man_vac: self.vacation.manual_vacation,
            vac_s: self.vacation.start_date,
            vac_e: self.vacation.end_date,
            ntp_ok: self.ntp_ok,
        }
    }
}

/// Helper to get current Singapore date as YYYYMMDD integer
pub fn current_date_ymd() -> i64 {
    let now = chrono::Utc::now() + chrono::Duration::hours(8); // SGT is UTC+8
    let year = chrono::Datelike::year(&now) as i64;
    let month = chrono::Datelike::month(&now) as i64;
    let day = chrono::Datelike::day(&now) as i64;
    year * 10000 + month * 100 + day
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub status: String,
    pub temp: u8,
    pub slot: u32,
    pub active_mins: u32,
    pub timer_secs: i64,
    pub bme_ok: bool,
    #[serde(rename = "rTemp")]
    pub r_temp: f32,
    #[serde(rename = "rHum")]
    pub r_hum: f32,
    pub vacation: bool,
    pub man_vac: bool,
    #[serde(rename = "vacS")]
    pub vac_s: i64,
    #[serde(rename = "vacE")]
    pub vac_e: i64,
    pub ntp_ok: bool,
}
