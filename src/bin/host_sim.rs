use ac_scheduler::ir::{DaikinCommand, IrTransmitter, MockTransmitter};
use ac_scheduler::scheduler::{SchedulerAction, SchedulerEngine};
use ac_scheduler::sensors::{MockSensor, TemperatureHumiditySensor};
use ac_scheduler::state::AppState;
use ac_scheduler::web::{parse_query_string, WebHandler};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("==================================================");
    log::info!("      Daikin AC Scheduler - Host Simulator        ");
    log::info!("==================================================");

    let state = Arc::new(Mutex::new(AppState::new()));
    let transmitter: Arc<Mutex<dyn IrTransmitter>> = Arc::new(Mutex::new(MockTransmitter::new()));
    let mut sensor = MockSensor::new(27.4, 68.0);
    let mut engine = SchedulerEngine::new();

    // Mark NTP as OK in simulator
    {
        let mut s = state.lock().unwrap();
        s.ntp_ok = true;
        s.bme_ok = true;
    }

    // Spawn 1-second scheduler background loop
    let sched_state = Arc::clone(&state);
    let sched_tx = Arc::clone(&transmitter);
    thread::spawn(move || {
        log::info!("[Scheduler] Background engine loop started.");

        loop {
            thread::sleep(Duration::from_secs(1));

            // 1. Check timer expiration
            let timer_expired = {
                let s = sched_state.lock().unwrap();
                if let Some(ref t) = s.timer {
                    t.start_instant.elapsed().as_secs() >= t.duration_secs
                } else {
                    false
                }
            };

            if timer_expired {
                log::info!("[Timer] Active timer expired. Turning AC OFF.");
                {
                    let mut s = sched_state.lock().unwrap();
                    s.manual_override = false;
                    s.set_off();
                }
                if let Ok(mut tx) = sched_tx.lock() {
                    let _ = tx.send_command(&DaikinCommand::off());
                }
            }

            // 2. Scheduler tick (Singapore Time UTC+8)
            let sgt_now = chrono::Utc::now() + chrono::Duration::hours(8);
            let hour = chrono::Timelike::hour(&sgt_now);
            let min = chrono::Timelike::minute(&sgt_now);

            let (vac_active, manual_override) = {
                let mut s = sched_state.lock().unwrap();
                s.current_slot = if ac_scheduler::scheduler::is_night_window(hour) {
                    1
                } else {
                    2
                };
                (s.is_vacation_active(), s.manual_override)
            };

            if let Some(action) = engine.tick(hour, min, vac_active, manual_override) {
                match action {
                    SchedulerAction::TurnOn(temp) => {
                        log::info!("[Scheduler] Auto Turning AC ON at {}°C (slot active)", temp);
                        {
                            let mut s = sched_state.lock().unwrap();
                            s.set_on(temp);
                        }
                        if let Ok(mut tx) = sched_tx.lock() {
                            let _ = tx.send_command(&DaikinCommand::cool_at(temp));
                        }
                    }
                    SchedulerAction::TurnOff => {
                        log::info!("[Scheduler] Auto Turning AC OFF (slot inactive)");
                        {
                            let mut s = sched_state.lock().unwrap();
                            s.set_off();
                        }
                        if let Ok(mut tx) = sched_tx.lock() {
                            let _ = tx.send_command(&DaikinCommand::off());
                        }
                    }
                    SchedulerAction::TurnOffAndClearOverride => {
                        log::info!("[Scheduler] Daytime boundary reached (07:00). Standby.");
                        {
                            let mut s = sched_state.lock().unwrap();
                            s.manual_override = false;
                            s.set_off();
                        }
                        if let Ok(mut tx) = sched_tx.lock() {
                            let _ = tx.send_command(&DaikinCommand::off());
                        }
                    }
                }
            }

            // 3. Update simulated sensor readings
            if let Ok((t, h)) = sensor.read() {
                let mut s = sched_state.lock().unwrap();
                s.room_temp = t;
                s.room_humidity = h;
            }
        }
    });

    // Start HTTP Server on port 8080
    let addr = "127.0.0.1:8080";
    let server = Server::http(addr).expect("Failed to bind HTTP server to 127.0.0.1:8080");
    log::info!("Dashboard running at: http://{}", addr);

    let handler = WebHandler::new(Arc::clone(&state), Arc::clone(&transmitter));

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let mut parts = url.splitn(2, '?');
        let path = parts.next().unwrap_or("/");
        let query_str = parts.next().unwrap_or("");
        let query = parse_query_string(query_str);

        let web_resp = handler.handle_get(path, &query);

        let header =
            Header::from_bytes(&b"Content-Type"[..], web_resp.content_type.as_bytes()).unwrap();
        let response = Response::from_string(web_resp.body)
            .with_status_code(web_resp.status_code)
            .with_header(header);

        let _ = request.respond(response);
    }
}
