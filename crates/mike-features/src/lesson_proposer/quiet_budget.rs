use crate::store::persistence::lessons_repo::LessonsPersist as _;
use chrono::{Timelike, Utc};
use serde_json::Value;

use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, PartialEq)]
pub struct QuietBudget {
    pub max_per_day: i64,
    pub start_hour_utc: u32,
    pub end_hour_utc: u32,
    pub idle_secs: i64,
}

impl Default for QuietBudget {
    fn default() -> Self {
        Self {
            max_per_day: 0,
            start_hour_utc: 22,
            end_hour_utc: 4,
            idle_secs: 300,
        }
    }
}

impl QuietBudget {
    pub fn apply_row(&mut self, key: &str, value: &Value) {
        let Some(value) = value.as_i64() else {
            return;
        };
        match key {
            "judge_lessons.quiet_max_per_day" => self.max_per_day = value.clamp(0, 1000),
            "judge_lessons.quiet_start_hour_utc" => self.start_hour_utc = value.clamp(0, 23) as u32,
            "judge_lessons.quiet_end_hour_utc" => self.end_hour_utc = value.clamp(0, 23) as u32,
            "judge_lessons.quiet_idle_secs" => self.idle_secs = value.clamp(60, 86400),
            _ => {}
        }
    }

    pub fn in_window(&self, hour: u32) -> bool {
        if self.start_hour_utc < self.end_hour_utc {
            hour >= self.start_hour_utc && hour < self.end_hour_utc
        } else if self.start_hour_utc > self.end_hour_utc {
            hour >= self.start_hour_utc || hour < self.end_hour_utc
        } else {
            false
        }
    }

    pub async fn effective_limit(&self, persist: &PersistHandle, normal: i64) -> i64 {
        let now = Utc::now();
        if self.max_per_day <= normal || !self.in_window(now.hour()) {
            return normal;
        }
        match persist
            .lessons()
            .has_recent_primary_activity(now - chrono::Duration::seconds(self.idle_secs))
            .await
        {
            Ok(false) => self.max_per_day,
            Ok(true) => normal,
            Err(error) => {
                tracing::warn!(%error, "lesson proposer quiet budget activity check failed; retaining daytime cap");
                normal
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn quiet_window_crosses_midnight_and_excludes_end() {
        let budget = QuietBudget::default();
        for hour in [22, 23, 0, 3] {
            assert!(budget.in_window(hour));
        }
        for hour in [4, 12, 21] {
            assert!(!budget.in_window(hour));
        }
    }

    #[test]
    fn equal_hours_disable_and_same_day_window_is_bounded() {
        let mut budget = QuietBudget {
            start_hour_utc: 4,
            end_hour_utc: 4,
            ..Default::default()
        };
        assert!(!(0..24).any(|hour| budget.in_window(hour)));
        budget.end_hour_utc = 8;
        assert!(budget.in_window(4));
        assert!(budget.in_window(7));
        assert!(!budget.in_window(3));
        assert!(!budget.in_window(8));
    }

    #[test]
    fn fresh_install_boost_is_disabled_and_inputs_are_bounded() {
        let mut budget = QuietBudget::default();
        assert_eq!(budget.max_per_day, 0);
        budget.apply_row("judge_lessons.quiet_max_per_day", &serde_json::json!(5000));
        budget.apply_row("judge_lessons.quiet_idle_secs", &serde_json::json!(0));
        assert_eq!(budget.max_per_day, 1000);
        assert_eq!(budget.idle_secs, 60);
    }
}
