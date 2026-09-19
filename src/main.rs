//! Main firmware entry point for ESP32 Daikin AC Scheduler.

#[cfg(not(target_os = "espidf"))]
fn main() {
    let config = ac_scheduler::config::AppConfig::load();
    println!("==================================================");
    println!("       AC Scheduler Firmware Configuration        ");
    println!("==================================================");
    println!("SSID:            {}", config.wifi.ssid);
    println!("Static IP:       {:?}", config.wifi.static_ip);
    println!("Gateway:         {:?}", config.wifi.gateway);
    println!("mDNS Hostname:   {}.local", config.wifi.mdns_hostname);
    println!("Fallback AP:     {}", config.wifi.ap_ssid);
    println!("--------------------------------------------------");
    println!("To run the local simulator and web dashboard on host:");
    println!("    cargo run --bin host-sim");
}

#[cfg(target_os = "espidf")]
use ac_scheduler::config::AppConfig;
#[cfg(target_os = "espidf")]
use ac_scheduler::ir::rmt::EspRmtTransmitter;
#[cfg(target_os = "espidf")]
use ac_scheduler::ir::{DaikinCommand, IrTransmitter};
#[cfg(target_os = "espidf")]
use ac_scheduler::scheduler::{is_night_window, SchedulerAction, SchedulerEngine};
#[cfg(target_os = "espidf")]
use ac_scheduler::sensors::bme280::EspBme280;
#[cfg(target_os = "espidf")]
use ac_scheduler::sensors::TemperatureHumiditySensor;
#[cfg(target_os = "espidf")]
use ac_scheduler::state::AppState;
#[cfg(target_os = "espidf")]
use ac_scheduler::storage::nvs::EspNvsStorage;
#[cfg(target_os = "espidf")]
use ac_scheduler::storage::{AppStorage, InMemoryStorage};
#[cfg(target_os = "espidf")]
use ac_scheduler::web::{parse_query_string, WebHandler};
#[cfg(target_os = "espidf")]
use esp_idf_hal::peripherals::Peripherals;
#[cfg(target_os = "espidf")]
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
#[cfg(target_os = "espidf")]
use esp_idf_svc::io::EspIOError;
#[cfg(target_os = "espidf")]
use esp_idf_svc::ipv4::{self, ClientSettings, Mask, Subnet};
#[cfg(target_os = "espidf")]
use esp_idf_svc::mdns::EspMdns;
#[cfg(target_os = "espidf")]
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::EspDefaultNvsPartition;
#[cfg(target_os = "espidf")]
use esp_idf_svc::ota::EspOta;
#[cfg(target_os = "espidf")]
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
#[cfg(target_os = "espidf")]
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
    EspWifi, WifiDriver,
};
#[cfg(target_os = "espidf")]
use std::net::Ipv4Addr;
#[cfg(target_os = "espidf")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "espidf")]
use std::thread::sleep;
#[cfg(target_os = "espidf")]
use std::time::Duration;

#[cfg(target_os = "espidf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Starting AC Scheduler firmware on ESP32...");
    let config = AppConfig::load();

    let peripherals = Peripherals::take()?;
    let sys_loop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs_default = EspDefaultNvsPartition::take()?;

    // 1. Configure Hardware Task Watchdog Timer (10s timeout, panic on expiry).
    // The main task is only subscribed right before the control loop, since Wi-Fi
    // connect/netif-up waits can each block for up to 15s.
    configure_task_watchdog();

    // Confirm running slot as valid (prevents automatic OTA rollback)
    if let Ok(mut ota) = EspOta::new() {
        if let Err(e) = ota.mark_running_slot_valid() {
            log::debug!("[OTA] Mark running slot valid: {:?}", e);
        }
    }

    // 2. Load NVS storage for Vacation Settings and Wi-Fi credentials
    let storage: Arc<Mutex<dyn AppStorage>> =
        match esp_idf_svc::nvs::EspNvs::new(nvs_default.clone(), "ac-prefs", true) {
            Ok(nvs) => Arc::new(Mutex::new(EspNvsStorage::new(nvs))),
            Err(e) => {
                log::warn!("Failed to open NVS: {:?}, using in-memory defaults", e);
                Arc::new(Mutex::new(InMemoryStorage::new()))
            }
        };

    let initial_vacation = storage.lock().unwrap().load().unwrap_or_default();
    let state = Arc::new(Mutex::new(AppState::new()));
    {
        let mut s = state.lock().unwrap();
        s.vacation = initial_vacation;
    }

    // 3. Initialize Modern ESP-IDF v5 RMT IR Transmitter on GPIO 1 (D1 on Seeed XIAO ESP32-C6)
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = match EspRmtTransmitter::new(1) {
        Ok(tx) => Arc::new(Mutex::new(tx)),
        Err(e) => {
            log::error!("Failed to initialize modern RMT on GPIO1: {:?}", e);
            Arc::new(Mutex::new(ac_scheduler::ir::MockTransmitter::new()))
        }
    };

    // 4. Initialize Modern ESP-IDF v5 I2C for BME280 sensor (SDA=GPIO22, SCL=GPIO23)
    let mut sensor = match EspBme280::new(22, 23) {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("Failed to initialize modern I2C for BME280: {:?}", e);
            None
        }
    };

    // 5. Configure Wi-Fi (Check NVS first, fallback to cfg.toml)
    let nvs_wifi = storage.lock().unwrap().load_wifi().unwrap_or(None);
    let (wifi_ssid, wifi_pass) = if let Some(ref creds) = nvs_wifi {
        log::info!("[Wi-Fi] Using credentials from NVS: {}", creds.ssid);
        (creds.ssid.clone(), creds.password.clone())
    } else {
        log::info!(
            "[Wi-Fi] Using credentials from cfg.toml: {}",
            config.wifi.ssid
        );
        (config.wifi.ssid.clone(), config.wifi.password.clone())
    };
    log::info!("Connecting to Wi-Fi SSID: {}", wifi_ssid);
    // Static IP (if configured) must be set when the STA netif is created
    let sta_netif = match static_ip_settings(&config) {
        Some(settings) => {
            log::info!(
                "[Netif] Applying Static IP {} (Gateway {})",
                settings.ip,
                settings.subnet.gateway
            );
            EspNetif::new_with_conf(&NetifConfiguration {
                ip_configuration: Some(ipv4::Configuration::Client(
                    ipv4::ClientConfiguration::Fixed(settings),
                )),
                ..NetifConfiguration::wifi_default_client()
            })?
        }
        None => EspNetif::new(NetifStack::Sta)?,
    };
    let esp_wifi = EspWifi::wrap_all(
        WifiDriver::new(peripherals.modem, sys_loop.clone(), Some(nvs_default))?,
        sta_netif,
        EspNetif::new(NetifStack::Ap)?,
    )?;

    let mut wifi = BlockingWifi::wrap(esp_wifi, sys_loop)?;

    let client_config = ClientConfiguration {
        ssid: wifi_ssid
            .as_str()
            .try_into()
            .map_err(|_| "wifi.ssid exceeds 32 bytes")?,
        password: wifi_pass
            .as_str()
            .try_into()
            .map_err(|_| "wifi.password exceeds 64 bytes")?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };
    let ap_config = AccessPointConfiguration {
        ssid: config
            .wifi
            .ap_ssid
            .as_str()
            .try_into()
            .map_err(|_| "wifi.ap_ssid exceeds 32 bytes")?,
        password: config
            .wifi
            .ap_password
            .as_str()
            .try_into()
            .map_err(|_| "wifi.ap_password exceeds 64 bytes")?,
        channel: 1,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };

    // Attempt Client connection
    let wifi_res = (|| -> Result<(), esp_idf_sys::EspError> {
        wifi.set_configuration(&Configuration::Client(client_config))?;
        wifi.start()?;
        wifi.connect()?;
        wifi.wait_netif_up()?;
        Ok(())
    })();

    let in_ap_mode = wifi_res.is_err();
    if in_ap_mode {
        log::warn!(
            "Failed to connect to '{}'. Starting fallback AP: {}",
            wifi_ssid,
            config.wifi.ap_ssid
        );
        wifi.set_configuration(&Configuration::AccessPoint(ap_config))?;
        wifi.start()?;
        wifi.wait_netif_up()?;

        // Start captive portal DNS server on UDP port 53 (redirects queries to 192.168.4.1)
        std::thread::spawn(|| {
            if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:53") {
                let mut buf = [0u8; 512];
                while let Ok((amt, src)) = socket.recv_from(&mut buf) {
                    if amt < 12 {
                        continue;
                    }
                    let mut response = Vec::with_capacity(amt + 16);
                    response.extend_from_slice(&buf[0..amt]);
                    response[2] = 0x81;
                    response[3] = 0x80;
                    response[6] = 0x00;
                    response[7] = 0x01;
                    response.extend_from_slice(&[
                        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04,
                        192, 168, 4, 1,
                    ]);
                    let _ = socket.send_to(&response, src);
                }
            }
        });
    }

    log::info!(
        "Wi-Fi is up! IP: {:?}",
        wifi.wifi().sta_netif().get_ip_info()
    );

    // 6. Initialize mDNS Hostname (http://ac-scheduler.local).
    // Kept alive for the rest of main: dropping EspMdns calls mdns_free() and stops the responder.
    let _mdns = match EspMdns::take() {
        Ok(mut mdns) => {
            match mdns.set_hostname(&config.wifi.mdns_hostname) {
                Ok(()) => log::info!(
                    "mDNS responder started: {}.local",
                    config.wifi.mdns_hostname
                ),
                Err(e) => log::warn!("Failed to set mDNS hostname: {:?}", e),
            }
            Some(mdns)
        }
        Err(e) => {
            log::warn!("Failed to start mDNS: {:?}", e);
            None
        }
    };

    // 7. Initialize SNTP for GMT+8 (Singapore Standard Time)
    log::info!("Starting SNTP synchronization for GMT+8...");
    let sntp = EspSntp::new_default()?;

    // 8. Start HTTP Web Server on Port 80
    log::info!("Starting HTTP Web Server on port 80...");
    let mut server = EspHttpServer::new(&HttpConfig::default())?;
    let handler = WebHandler::with_all_storage(
        Arc::clone(&state),
        Arc::clone(&transmitter),
        storage.clone(),
        Some(storage.clone()),
    );

    let h = Arc::new(handler);
    let routes = [
        "/",
        "/status",
        "/cmd",
        "/timer",
        "/vacation_toggle",
        "/schedule",
        "/reset-wifi",
        "/wifi",
        "/wifi_save",
        "/update",
        "/hotspot-detect.html",
        "/generate_204",
        "/ncsi.txt",
    ];
    for route in routes {
        let h_clone = Arc::clone(&h);
        server.fn_handler(
            route,
            esp_idf_svc::http::Method::Get,
            move |req| -> Result<(), EspIOError> {
                let uri = req.uri();
                let mut parts = uri.splitn(2, '?');
                let path = parts.next().unwrap_or("/");
                let query_str = parts.next().unwrap_or("");
                let query = parse_query_string(query_str);

                let res = h_clone.handle_get(path, &query);
                let mut response = req.into_response(
                    res.status_code,
                    None,
                    &[("Content-Type", res.content_type)],
                )?;
                response.write(res.body.as_bytes())?;
                Ok(())
            },
        )?;
    }

    // POST /update: Stream uploaded firmware binary directly to the inactive OTA partition
    server.fn_handler(
        "/update",
        esp_idf_svc::http::Method::Post,
        move |mut req| -> Result<(), EspIOError> {
            log::info!("[OTA] Received firmware update request...");
            let mut ota = match EspOta::new() {
                Ok(o) => o,
                Err(e) => {
                    let mut resp = req.into_status_response(500)?;
                    resp.write(format!("Failed to initialize OTA: {e:?}").as_bytes())?;
                    return Ok(());
                }
            };

            let mut update = match ota.initiate_update() {
                Ok(u) => u,
                Err(e) => {
                    let mut resp = req.into_status_response(500)?;
                    resp.write(format!("Failed to initiate OTA: {e:?}").as_bytes())?;
                    return Ok(());
                }
            };

            let mut buf = [0u8; 1024];
            let mut total_bytes = 0;
            loop {
                let n = req.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                if let Err(e) = update.write(&buf[..n]) {
                    let _ = update.abort();
                    let mut resp = req.into_status_response(500)?;
                    resp.write(format!("Failed to write OTA chunk: {e:?}").as_bytes())?;
                    return Ok(());
                }
                total_bytes += n;
            }

            if total_bytes == 0 {
                let _ = update.abort();
                let mut resp = req.into_status_response(400)?;
                resp.write(b"Empty firmware payload")?;
                return Ok(());
            }

            if let Err(e) = update.complete() {
                let mut resp = req.into_status_response(500)?;
                resp.write(format!("Failed to complete OTA: {e:?}").as_bytes())?;
                return Ok(());
            }

            log::info!(
                "[OTA] Successfully flashed {} bytes! Rebooting in 2s...",
                total_bytes
            );
            let mut resp = req.into_status_response(200)?;
            resp.write(b"OTA update successful! Rebooting...")?;

            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                unsafe {
                    esp_idf_sys::esp_restart();
                }
            });

            Ok(())
        },
    )?;

    // 9. Post-Boot Recovery Evaluation
    let mut engine = SchedulerEngine::new();
    let sgt_now = chrono::Utc::now() + chrono::Duration::hours(8);
    let hour = chrono::Timelike::hour(&sgt_now);
    let min = chrono::Timelike::minute(&sgt_now);

    let vac_active = state.lock().unwrap().is_vacation_active();
    if let Some(action) = engine.evaluate_boot_recovery(hour, min, vac_active) {
        if let SchedulerAction::TurnOn(temp) = action {
            log::info!("[Boot Recovery] Immediately turning AC ON at {}°C", temp);
            state.lock().unwrap().set_on(temp);
            if let Ok(mut tx) = transmitter.lock() {
                let _ = tx.send_command(&DaikinCommand::cool_at(temp));
            }
        }
    }

    // 10. Main Control Loop (subscribe this task to the watchdog; the loop pets it every second)
    if let Err(e) =
        esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_add(std::ptr::null_mut()) })
    {
        log::warn!("Failed to subscribe main task to watchdog: {:?}", e);
    }
    loop {
        sleep(Duration::from_secs(1));

        // Pet the watchdog timer
        unsafe {
            let _ = esp_idf_sys::esp_task_wdt_reset();
        }

        // Check NTP status. ESP-IDF reports Completed only once per sync, then resets the
        // status (esp_sntp.h), so latch it instead of mirroring the per-poll value.
        if sntp.get_sync_status() == SyncStatus::Completed {
            state.lock().unwrap().ntp_ok = true;
        }

        // Check active timer
        let timer_expired = state.lock().unwrap().expire_timer();
        if timer_expired {
            log::info!("[Timer] Countdown expired. Turning AC OFF.");
            if let Ok(mut tx) = transmitter.lock() {
                let _ = tx.send_command(&DaikinCommand::off());
            }
        }

        // Ticks scheduler
        let sgt_now = chrono::Utc::now() + chrono::Duration::hours(8);
        let hour = chrono::Timelike::hour(&sgt_now);
        let min = chrono::Timelike::minute(&sgt_now);

        let (vac_active, manual_override) = {
            let mut s = state.lock().unwrap();
            s.current_slot = if is_night_window(hour) { 1 } else { 2 };
            (s.is_vacation_active(), s.manual_override)
        };

        if let Some(action) = engine.tick(hour, min, vac_active, manual_override) {
            match action {
                SchedulerAction::TurnOn(temp) => {
                    log::info!("[Scheduler] Auto slot ON ({}°C)", temp);
                    state.lock().unwrap().set_on(temp);
                    if let Ok(mut tx) = transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::cool_at(temp));
                    }
                }
                SchedulerAction::TurnOff => {
                    log::info!("[Scheduler] Auto slot OFF");
                    state.lock().unwrap().set_off();
                    if let Ok(mut tx) = transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::off());
                    }
                }
                SchedulerAction::TurnOffAndClearOverride => {
                    log::info!("[Scheduler] 07:00 Daytime Standby. Off.");
                    {
                        let mut s = state.lock().unwrap();
                        s.manual_override = false;
                        s.set_off();
                    }
                    if let Ok(mut tx) = transmitter.lock() {
                        let _ = tx.send_command(&DaikinCommand::off());
                    }
                }
            }
        }

        // Read BME280 sensor
        if let Some(Ok((t, h))) = sensor.as_mut().map(|s| s.read()) {
            let mut s = state.lock().unwrap();
            s.room_temp = t;
            s.room_humidity = h;
            s.bme_ok = true;
        }
    }
}

#[cfg(target_os = "espidf")]
const WDT_TIMEOUT_MS: u32 = 10_000;

/// Sets a 10s panicking Task WDT. ESP-IDF v5 already initializes the TWDT at boot
/// (CONFIG_ESP_TASK_WDT_INIT=y by default), so reconfigure it; init only if that is disabled.
#[cfg(target_os = "espidf")]
fn configure_task_watchdog() {
    let wdt_config = esp_idf_sys::esp_task_wdt_config_t {
        timeout_ms: WDT_TIMEOUT_MS,
        idle_core_mask: 1, // watch CPU0's idle task (ESP32-C6 is single-core)
        trigger_panic: true,
    };
    let res = match esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_reconfigure(&wdt_config) })
    {
        Err(e) if e.code() == esp_idf_sys::ESP_ERR_INVALID_STATE => {
            esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_init(&wdt_config) })
        }
        other => other,
    };
    if let Err(e) = res {
        log::warn!("Failed to configure task watchdog: {:?}", e);
    }
}

/// Builds fixed STA IP settings when static_ip, gateway and subnet are all configured.
/// Returns None (DHCP) when any is missing, logging a warning if they are present but invalid.
#[cfg(target_os = "espidf")]
fn static_ip_settings(config: &AppConfig) -> Option<ClientSettings> {
    let ip_str = config.wifi.static_ip.as_deref()?;
    let gw_str = config.wifi.gateway.as_deref()?;
    let mask_str = config.wifi.subnet.as_deref()?;

    let parsed = (|| {
        let ip: Ipv4Addr = ip_str.parse().ok()?;
        let gateway: Ipv4Addr = gw_str.parse().ok()?;
        let mask = Mask::try_from(mask_str.parse::<Ipv4Addr>().ok()?).ok()?;
        Some(ClientSettings {
            ip,
            subnet: Subnet { gateway, mask },
            dns: Some(gateway),
            secondary_dns: Some(Ipv4Addr::new(8, 8, 8, 8)),
        })
    })();

    if parsed.is_none() {
        log::warn!(
            "[Netif] Invalid static IP config (ip={}, gateway={}, subnet={}); using DHCP",
            ip_str,
            gw_str,
            mask_str
        );
    }
    parsed
}
