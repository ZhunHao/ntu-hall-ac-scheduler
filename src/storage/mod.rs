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

/// Combined storage trait for vacation and Wi-Fi credentials.
pub trait AppStorage: VacationStorage + WifiStorage {}
impl<T: VacationStorage + WifiStorage> AppStorage for T {}

/// In-memory storage for host simulator and unit tests.
#[derive(Default, Clone)]
pub struct InMemoryStorage {
    settings: VacationSettings,
    wifi: Option<WifiCredentials>,
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
