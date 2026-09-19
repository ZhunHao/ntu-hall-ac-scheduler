use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG_STR: &str = include_str!("../cfg.toml");

fn default_mdns_hostname() -> String {
    "ac-scheduler".to_string()
}

fn default_ap_ssid() -> String {
    "AC-Scheduler-Setup".to_string()
}

fn default_ap_password() -> String {
    "acsetup01".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiSettings {
    pub ssid: String,
    pub password: String,
    pub static_ip: Option<String>,
    pub gateway: Option<String>,
    pub subnet: Option<String>,
    pub dns: Option<String>,
    #[serde(default = "default_mdns_hostname")]
    pub mdns_hostname: String,
    #[serde(default = "default_ap_ssid")]
    pub ap_ssid: String,
    #[serde(default = "default_ap_password")]
    pub ap_password: String,
}

impl Default for WifiSettings {
    fn default() -> Self {
        Self {
            ssid: "Your_WiFi_SSID".to_string(),
            password: "Your_WiFi_Password".to_string(),
            static_ip: Some("192.168.0.93".to_string()),
            gateway: Some("192.168.0.1".to_string()),
            subnet: Some("255.255.255.0".to_string()),
            dns: Some("192.168.0.1".to_string()),
            mdns_hostname: default_mdns_hostname(),
            ap_ssid: default_ap_ssid(),
            ap_password: default_ap_password(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub wifi: WifiSettings,
}

impl AppConfig {
    pub fn load() -> Self {
        toml::from_str(DEFAULT_CONFIG_STR).unwrap_or_else(|e| {
            log::warn!("Failed to parse cfg.toml: {}, using defaults", e);
            AppConfig::default()
        })
    }
}
