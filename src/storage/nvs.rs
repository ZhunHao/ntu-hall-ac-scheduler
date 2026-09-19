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

    #[cfg(target_os = "espidf")]
    fn load_date(&self, key: &str) -> Result<i64, String> {
        // Arduino Preferences::putLong writes i32. Also accept dates written by
        // the initial Rust port, which used i64 under the same keys.
        match self.nvs.get_i32(key) {
            Ok(value) => Ok(i64::from(value.unwrap_or(0))),
            Err(e) if e.code() == esp_idf_sys::ESP_ERR_NVS_TYPE_MISMATCH => self
                .nvs
                .get_i64(key)
                .map(|value| value.unwrap_or(0))
                .map_err(|e| format!("Failed to read {key}: {e}")),
            Err(e) => Err(format!("Failed to read {key}: {e}")),
        }
    }

    #[cfg(target_os = "espidf")]
    fn active_record(&self) -> Result<Option<u8>, String> {
        match self.nvs.get_u8("vacActive").map_err(|e| e.to_string())? {
            None => Ok(None),
            Some(slot @ (0 | 1)) => Ok(Some(slot)),
            Some(_) => Err("Invalid vacation record selector".to_string()),
        }
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
            // The selected versioned record is authoritative after the first save.
            // The JSON tuple [manual, start, end] fits in 64 bytes even for i64 bounds.
            let mut buffer = [0u8; 64];
            if let Some(slot) = self.active_record()? {
                let key = if slot == 0 { "vacV1a" } else { "vacV1b" };
                let bytes = self
                    .nvs
                    .get_blob(key, &mut buffer)
                    .map_err(|e| e.to_string())?
                    .ok_or("Missing active vacation record")?;
                let (manual_vacation, start_date, end_date): (bool, i64, i64) =
                    serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
                return Ok(VacationSettings {
                    manual_vacation,
                    start_date,
                    end_date,
                });
            }

            // Migrate existing Arduino/Rust keys lazily on the next save.
            let man_vac = self
                .nvs
                .get_u8("manVac")
                .map_err(|e| e.to_string())?
                .unwrap_or(0)
                != 0;
            let vac_s = self.load_date("vacS")?;
            let vac_e = self.load_date("vacE")?;
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
            let bytes = serde_json::to_vec(&(
                settings.manual_vacation,
                settings.start_date,
                settings.end_date,
            ))
            .map_err(|e| e.to_string())?;
            // esp-idf-svc 0.51's set_blob erases before writing. Only overwrite the
            // inactive slot, then publish it with a single non-erasing u8 write.
            // A failed blob write leaves the previous selected record intact.
            let next = match self.active_record()? {
                Some(0) => 1,
                _ => 0,
            };
            let key = if next == 0 { "vacV1a" } else { "vacV1b" };
            self.nvs.set_blob(key, &bytes).map_err(|e| e.to_string())?;
            self.nvs
                .set_u8("vacActive", next)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        #[cfg(not(target_os = "espidf"))]
        {
            let _ = settings;
            Ok(())
        }
    }
}
