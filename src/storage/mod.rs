pub mod nvs;

use crate::scheduler::VacationSettings;

pub trait VacationStorage: Send + Sync {
    fn load(&self) -> Result<VacationSettings, String>;
    fn save(&mut self, settings: &VacationSettings) -> Result<(), String>;
}

/// In-memory storage for host simulator and unit tests.
#[derive(Default)]
pub struct InMemoryStorage {
    settings: VacationSettings,
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
