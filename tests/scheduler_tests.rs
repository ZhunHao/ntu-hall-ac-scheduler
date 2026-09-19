use ac_scheduler::scheduler::{
    is_night_window, should_ac_be_on, SchedulerAction, SchedulerEngine, VacationSettings,
};
use ac_scheduler::state::AppState;

#[test]
fn timer_expiry_keeps_ac_off_until_morning_override_reset() {
    let mut state = AppState::new();
    let mut engine = SchedulerEngine::new();
    state.start_timer(30);
    assert!(!state.expire_timer());
    assert_eq!(engine.tick(21, 50, false, state.manual_override), None);
    assert_eq!(engine.tick(22, 19, false, state.manual_override), None);

    state.timer.as_mut().unwrap().start_instant =
        std::time::Instant::now() - std::time::Duration::from_secs(1801);
    assert!(state.expire_timer());
    assert!(state.timer.is_none());
    assert_eq!(state.system_status, "Standby");
    assert_eq!(engine.tick(22, 20, false, state.manual_override), None);
    assert!(!state.expire_timer());
    assert_eq!(engine.tick(23, 15, false, state.manual_override), None);
    assert_eq!(
        engine.tick(7, 0, false, state.manual_override),
        Some(SchedulerAction::TurnOffAndClearOverride)
    );
    state.manual_override = false;
    assert_eq!(
        engine.tick(22, 0, false, state.manual_override),
        Some(SchedulerAction::TurnOn(16))
    );
}

#[test]
fn test_night_window_detection() {
    assert!(is_night_window(22));
    assert!(is_night_window(23));
    assert!(is_night_window(0));
    assert!(is_night_window(1));
    assert!(is_night_window(6));
    assert!(!is_night_window(7));
    assert!(!is_night_window(8));
    assert!(!is_night_window(12));
    assert!(!is_night_window(21));
}

#[test]
fn test_schedule_slot_boundaries() {
    // Slot 1: 22:00 - 22:45
    assert!(!should_ac_be_on(21 * 60 + 59));
    assert!(should_ac_be_on(22 * 60));
    assert!(should_ac_be_on(22 * 60 + 30));
    assert!(!should_ac_be_on(22 * 60 + 45));

    // Between Slot 1 & 2: 22:45 - 23:15
    assert!(!should_ac_be_on(23 * 60));

    // Slot 2: 23:15 - 23:40
    assert!(should_ac_be_on(23 * 60 + 15));
    assert!(should_ac_be_on(23 * 60 + 39));
    assert!(!should_ac_be_on(23 * 60 + 40));

    // Slot 3: 00:15 - 00:35
    assert!(should_ac_be_on(15));
    assert!(!should_ac_be_on(35));

    // Slot 4: 01:20 - 01:40
    assert!(should_ac_be_on(60 + 20));
    assert!(!should_ac_be_on(60 + 40));

    // Slot 5: 02:40 - 03:00
    assert!(should_ac_be_on(2 * 60 + 40));
    assert!(!should_ac_be_on(3 * 60));

    // Slot 6: 04:15 - 04:40
    assert!(should_ac_be_on(4 * 60 + 15));
    assert!(!should_ac_be_on(4 * 60 + 40));

    // Slot 7: 05:20 - 06:00
    assert!(should_ac_be_on(5 * 60 + 20));
    assert!(!should_ac_be_on(6 * 60));

    // After last slot: 06:00 - 07:00
    assert!(!should_ac_be_on(6 * 60 + 30));
}

#[test]
fn test_scheduler_engine_state_transitions() {
    let mut engine = SchedulerEngine::new();

    // 21:59: Not in window yet
    let act = engine.tick(21, 59, false, false);
    assert_eq!(act, None);
    assert!(!engine.in_active_window);

    // 22:00: Window begins, Slot 1 begins -> Turn ON at 16°C
    let act = engine.tick(22, 0, false, false);
    assert_eq!(act, Some(SchedulerAction::TurnOn(16)));
    assert!(engine.in_active_window);
    assert!(engine.last_cycle_was_on);

    // 22:01: Still in slot 1 -> No duplicate send
    let act = engine.tick(22, 1, false, false);
    assert_eq!(act, None);

    // 22:45: Slot 1 ends -> Turn OFF
    let act = engine.tick(22, 45, false, false);
    assert_eq!(act, Some(SchedulerAction::TurnOff));
    assert!(!engine.last_cycle_was_on);

    // 07:00: Active window ends -> Turn OFF and clear override
    let act = engine.tick(7, 0, false, false);
    assert_eq!(act, Some(SchedulerAction::TurnOffAndClearOverride));
    assert!(!engine.in_active_window);
}

#[test]
fn test_vacation_mode_suppression() {
    let mut engine = SchedulerEngine::new();
    let vac = VacationSettings::new(true, 0, 0);

    // At 22:00, vacation is active -> should NOT turn on
    let act = engine.tick(22, 0, vac.is_active(20260918), false);
    assert_eq!(act, None);
}

#[test]
fn test_boot_recovery() {
    let mut engine = SchedulerEngine::new();

    // Boot at 22:10 (inside slot 1) -> immediately turns on at 16°C
    let act = engine.evaluate_boot_recovery(22, 10, false);
    assert_eq!(act, Some(SchedulerAction::TurnOn(16)));
    assert!(engine.in_active_window);
    assert!(engine.last_cycle_was_on);

    // Boot at 14:00 (daytime) -> no action
    let mut daytime_engine = SchedulerEngine::new();
    let act = daytime_engine.evaluate_boot_recovery(14, 0, false);
    assert_eq!(act, None);
    assert!(!daytime_engine.in_active_window);
}
