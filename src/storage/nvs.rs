//! ESP32 Non-Volatile Storage (NVS) for vacation settings persistence.

use super::VacationStorage;
use crate::scheduler::VacationSettings;

#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::EspNvs;

pub struct EspNvsStorage {
    #[cfg(target_os = "espidf")]
    nvs: EspNvs<esp_idf_svc::nvs::NvsDefault>,
    #[cfg(not(target_os = "espidf"))]
    _dummy: (),
}

impl EspNvsStorage {
    #[cfg(target_os = "espidf")]
    pub fn new(nvs: EspNvs<esp_idf_svc::nvs::NvsDefault>) -> Self {
        Self { nvs }
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn new() -> Self {
        Self { _dummy: () }
    }
}

#[cfg(not(target_os = "espidf"))]
impl Default for EspNvsStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl VacationStorage for EspNvsStorage {
    fn load(&self) -> Result<VacationSettings, String> {
        #[cfg(target_os = "espidf")]
        {
            let man_vac = self.nvs.get_u8("manVac").unwrap_or(Some(0)).unwrap_or(0) != 0;
            let vac_s = self.nvs.get_i64("vacS").unwrap_or(Some(0)).unwrap_or(0);
            let vac_e = self.nvs.get_i64("vacE").unwrap_or(Some(0)).unwrap_or(0);
            Ok(VacationSettings {
                manual_vacation: man_vac,
                start_date: vac_s,
                end_date: vac_e,
            })
        }
        #[cfg(not(target_os = "espidf"))]
        {
            Ok(VacationSettings::default())
        }
    }

    fn save(&mut self, settings: &VacationSettings) -> Result<(), String> {
        #[cfg(target_os = "espidf")]
        {
            let val = if settings.manual_vacation { 1u8 } else { 0u8 };
            let _ = self.nvs.set_u8("manVac", val);
            let _ = self.nvs.set_i64("vacS", settings.start_date);
            let _ = self.nvs.set_i64("vacE", settings.end_date);
            Ok(())
        }
        #[cfg(not(target_os = "espidf"))]
        {
            let _ = settings;
            Ok(())
        }
    }
}
