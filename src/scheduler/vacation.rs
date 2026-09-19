//! Vacation mode management.
//!
//! Supports manual toggle and calendar date-range pauses. Persisted in non-volatile storage.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VacationSettings {
    pub manual_vacation: bool,
    pub start_date: i64, // Format: YYYYMMDD (e.g. 20260918), 0 if unset
    pub end_date: i64,   // Format: YYYYMMDD, 0 if unset
}

impl VacationSettings {
    pub fn new(manual: bool, start: i64, end: i64) -> Self {
        Self {
            manual_vacation: manual,
            start_date: start,
            end_date: end,
        }
    }

    /// Evaluates if vacation mode is currently active given the current date (YYYYMMDD).
    pub fn is_active(&self, today_ymd: i64) -> bool {
        if self.manual_vacation {
            return true;
        }

        if self.start_date > 0 && self.end_date > 0 {
            today_ymd >= self.start_date && today_ymd <= self.end_date
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.manual_vacation = false;
        self.start_date = 0;
        self.end_date = 0;
    }
}
