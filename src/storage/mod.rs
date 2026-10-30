pub mod nvs;

use crate::scheduler::VacationSettings;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

pub trait VacationStorage: Send + Sync {
    fn load(&self) -> Result<VacationSettings, String>;
    fn save(&mut self, settings: &VacationSettings) -> Result<(), String>;
}

pub trait WifiStorage: Send + Sync {
    fn load_wifi(&self) -> Result<Option<WifiCredentials>, String>;
    fn save_wifi(&mut self, creds: &WifiCredentials) -> Result<(), String>;
    fn clear_wifi(&mut self) -> Result<(), String>;
}

/// Last wall-clock time seen before a reset (Unix seconds). Diagnostic only: it tells how long
/// the power was out, and is never used to set the clock or run the schedule.
pub trait TimeStorage: Send + Sync {
    fn load_last_known_time(&self) -> Result<Option<i64>, String>;
    fn save_last_known_time(&mut self, unix_secs: i64) -> Result<(), String>;
}

/// Combined storage trait for vacation, Wi-Fi credentials and the last-known time.
pub trait AppStorage: VacationStorage + WifiStorage + TimeStorage {}
impl<T: VacationStorage + WifiStorage + TimeStorage> AppStorage for T {}

/// In-memory storage for host simulator and unit tests.
#[derive(Default, Clone)]
pub struct InMemoryStorage {
    settings: VacationSettings,
    wifi: Option<WifiCredentials>,
    last_known_time: Option<i64>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VacationStorage for InMemoryStorage {
    fn load(&self) -> Result<VacationSettings, String> {
        Ok(self.settings)
    }

    fn save(&mut self, settings: &VacationSettings) -> Result<(), String> {
        self.settings = *settings;
        Ok(())
    }
}

impl WifiStorage for InMemoryStorage {
    fn load_wifi(&self) -> Result<Option<WifiCredentials>, String> {
        Ok(self.wifi.clone())
    }

    fn save_wifi(&mut self, creds: &WifiCredentials) -> Result<(), String> {
        self.wifi = Some(creds.clone());
        Ok(())
    }

    fn clear_wifi(&mut self) -> Result<(), String> {
        self.wifi = None;
        Ok(())
    }
}

impl TimeStorage for InMemoryStorage {
    fn load_last_known_time(&self) -> Result<Option<i64>, String> {
        Ok(self.last_known_time)
    }

    fn save_last_known_time(&mut self, unix_secs: i64) -> Result<(), String> {
        self.last_known_time = Some(unix_secs);
        Ok(())
    }
}
