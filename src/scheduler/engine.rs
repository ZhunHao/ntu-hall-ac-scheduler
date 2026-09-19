//! Core Scheduler Engine.
//!
//! Implements the 7-slot nocturnal schedule for Singapore's ambient temperature profile,
//! tracking active windows, manual overrides, and vacation states.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleSlot {
    pub start_min: u32,
    pub end_min: u32,
}

/// 7 asymmetric cycles shaped to Singapore's nocturnal temperature curve (22:00 - 07:00).
#[allow(clippy::erasing_op, clippy::identity_op)]
pub const SCHEDULE: [CycleSlot; 7] = [
    CycleSlot {
        start_min: 22 * 60,
        end_min: 22 * 60 + 45,
    }, // Thermal Purge    22:00-22:45 (45m)
    CycleSlot {
        start_min: 23 * 60 + 15,
        end_min: 23 * 60 + 40,
    }, // Sleep Onset      23:15-23:40 (25m)
    CycleSlot {
        start_min: 0 * 60 + 15,
        end_min: 0 * 60 + 35,
    }, // Deep Sleep Entry 00:15-00:35 (20m)
    CycleSlot {
        start_min: 1 * 60 + 20,
        end_min: 1 * 60 + 40,
    }, // Deep Sleep Hold  01:20-01:40 (20m)
    CycleSlot {
        start_min: 2 * 60 + 40,
        end_min: 3 * 60,
    }, // Cold Window      02:40-03:00 (20m)
    CycleSlot {
        start_min: 4 * 60 + 15,
        end_min: 4 * 60 + 40,
    }, // Pre-Dawn Prep    04:15-04:40 (25m)
    CycleSlot {
        start_min: 5 * 60 + 20,
        end_min: 6 * 60,
    }, // Sunrise Fort     05:20-06:00 (40m)
];

pub fn is_night_window(hour: u32) -> bool {
    !(7..22).contains(&hour)
}

pub fn should_ac_be_on(mins_since_midnight: u32) -> bool {
    for slot in &SCHEDULE {
        if mins_since_midnight >= slot.start_min && mins_since_midnight < slot.end_min {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerAction {
    TurnOn(u8),
    TurnOff,
    TurnOffAndClearOverride,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerEngine {
    pub in_active_window: bool,
    pub last_cycle_was_on: bool,
}

impl SchedulerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates immediate state upon boot recovery.
    pub fn evaluate_boot_recovery(
        &mut self,
        hour: u32,
        min: u32,
        vacation_active: bool,
    ) -> Option<SchedulerAction> {
        if is_night_window(hour) && !vacation_active {
            self.in_active_window = true;
            let m = hour * 60 + min;
            if should_ac_be_on(m) {
                self.last_cycle_was_on = true;
                return Some(SchedulerAction::TurnOn(16));
            }
        }
        None
    }

    /// Evaluates the periodic 1-second scheduler tick.
    pub fn tick(
        &mut self,
        hour: u32,
        min: u32,
        vacation_active: bool,
        manual_override: bool,
    ) -> Option<SchedulerAction> {
        let in_window = is_night_window(hour);

        if in_window {
            let m = hour * 60 + min;
            let should_on = should_ac_be_on(m);

            if !vacation_active && !manual_override {
                if should_on && !self.last_cycle_was_on {
                    self.last_cycle_was_on = true;
                    self.in_active_window = true;
                    return Some(SchedulerAction::TurnOn(16));
                } else if !should_on && self.last_cycle_was_on {
                    self.last_cycle_was_on = false;
                    self.in_active_window = true;
                    return Some(SchedulerAction::TurnOff);
                }
            } else if vacation_active {
                // Keep track of schedule position even while vacation suppresses transmission
                self.last_cycle_was_on = should_on;
            }

            self.in_active_window = true;
            None
        } else {
            // Daytime standby (07:00 - 22:00)
            if self.in_active_window {
                self.in_active_window = false;
                self.last_cycle_was_on = false;
                // At 07:00 boundary, shut off AC and clear any manual override
                return Some(SchedulerAction::TurnOffAndClearOverride);
            }
            None
        }
    }
}
