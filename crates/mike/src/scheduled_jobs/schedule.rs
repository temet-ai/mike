use chrono::{DateTime, Datelike, Days, Duration, NaiveDate, Timelike, Utc, Weekday};

const BACKOFF_BASE: Duration = Duration::minutes(30);
const BACKOFF_CAP: Duration = Duration::hours(4);
const MAX_DOUBLINGS: u32 = 6;
const SEARCH_DAYS: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Period {
    Hour,
    Day,
    Week,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Cadence {
    pub(super) weekday: Option<Weekday>,
    pub(super) hours: &'static [u32],
    pub(super) minute: u32,
    pub(super) period: Period,
}

type PeriodKey = (i32, u32, u32);

impl Cadence {
    fn is_open(&self, now: DateTime<Utc>) -> bool {
        self.weekday.is_none_or(|day| now.weekday() == day)
            && self.hours.contains(&now.hour())
            && now.minute() >= self.minute
    }

    fn period_key(&self, now: DateTime<Utc>) -> PeriodKey {
        match self.period {
            Period::Hour => (now.year(), now.ordinal(), now.hour()),
            Period::Day => (now.year(), now.ordinal(), 0),
            Period::Week => {
                let week = now.iso_week();
                (week.year(), week.week(), 0)
            }
        }
    }

    pub(super) fn interval(&self) -> Duration {
        match self.period {
            Period::Hour => Duration::hours(1),
            Period::Day => Duration::hours(24),
            Period::Week => Duration::days(7),
        }
    }

    fn due_instants_on(&self, date: NaiveDate) -> Vec<DateTime<Utc>> {
        if self.weekday.is_some_and(|day| date.weekday() != day) {
            return Vec::new();
        }
        let hours: Vec<u32> = match self.period {
            Period::Hour => self.hours.to_vec(),
            Period::Day | Period::Week => self.hours.iter().copied().min().into_iter().collect(),
        };
        hours
            .into_iter()
            .filter_map(|hour| date.and_hms_opt(hour, self.minute, 0))
            .map(|at| at.and_utc())
            .collect()
    }

    pub(super) fn next_due_after(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        (0..=SEARCH_DAYS)
            .filter_map(|ahead| now.date_naive().checked_add_days(Days::new(ahead)))
            .flat_map(|date| self.due_instants_on(date))
            .filter(|at| *at > now)
            .min()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct Schedule {
    last_attempt_period: Option<PeriodKey>,
    consecutive_failures: u32,
    retry_after: Option<DateTime<Utc>>,
}

pub(super) fn backoff_after(consecutive_failures: u32) -> Duration {
    let doublings = consecutive_failures.saturating_sub(1).min(MAX_DOUBLINGS);
    (BACKOFF_BASE * (1_i32 << doublings)).min(BACKOFF_CAP)
}

impl Schedule {
    pub(super) fn restored(
        cadence: &Cadence,
        last_scheduled_start: Option<DateTime<Utc>>,
        consecutive_failures: u32,
        retry_after: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            last_attempt_period: last_scheduled_start.map(|at| cadence.period_key(at)),
            consecutive_failures,
            retry_after,
        }
    }

    pub(super) fn finish_manual(&mut self, finished_at: DateTime<Utc>, succeeded: bool) {
        if succeeded {
            self.finish(finished_at, true);
            return;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    pub(super) fn due(&self, cadence: &Cadence, now: DateTime<Utc>) -> bool {
        cadence.is_open(now)
            && self.last_attempt_period != Some(cadence.period_key(now))
            && self.retry_after.is_none_or(|not_before| now >= not_before)
    }

    pub(super) fn begin(&mut self, cadence: &Cadence, now: DateTime<Utc>) {
        self.last_attempt_period = Some(cadence.period_key(now));
    }

    pub(super) fn finish(&mut self, finished_at: DateTime<Utc>, succeeded: bool) {
        if succeeded {
            self.consecutive_failures = 0;
            self.retry_after = None;
            return;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_after = Some(finished_at + backoff_after(self.consecutive_failures));
    }

    pub(super) fn next_run_at(
        &self,
        cadence: &Cadence,
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        let from = self
            .retry_after
            .map_or(now, |not_before| not_before.max(now));
        let open_and_unattempted =
            cadence.is_open(from) && self.last_attempt_period != Some(cadence.period_key(from));
        if open_and_unattempted {
            Some(from)
        } else {
            cadence.next_due_after(from)
        }
    }

    pub(super) fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub(super) fn retry_after(&self) -> Option<DateTime<Utc>> {
        self.retry_after
    }
}

#[cfg(test)]
#[path = "schedule_restore_tests.rs"]
mod schedule_restore_tests;

#[cfg(test)]
mod tests {
    use super::super::jobs::{
        AUDIT, CARGO_SWEEP, COMPRESSOR_LOOP, LESSONS_COMMIT, PROMOTE, USAGE_AUDIT,
    };
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn compressor_loop_runs_once_a_day_in_its_morning_window() {
        let mut schedule = Schedule::default();
        let cadence = &COMPRESSOR_LOOP.cadence;
        assert!(!schedule.due(cadence, at(14, 4, 29)));
        assert!(schedule.due(cadence, at(14, 4, 30)));
        assert!(schedule.due(cadence, at(14, 6, 59)));
        assert!(!schedule.due(cadence, at(14, 7, 0)));
        schedule.begin(cadence, at(14, 4, 31));
        schedule.finish(at(14, 4, 40), true);
        assert!(!schedule.due(cadence, at(14, 5, 30)));
        assert!(schedule.due(cadence, at(15, 4, 30)));
    }

    #[test]
    fn cargo_sweep_runs_on_the_hour_every_six_hours() {
        let schedule = Schedule::default();
        let cadence = &CARGO_SWEEP.cadence;
        for hour in [0, 6, 12, 18] {
            assert!(schedule.due(cadence, at(14, hour, 0)), "hour {hour}");
        }
        for hour in [1, 5, 7, 11, 13, 17, 19, 23] {
            assert!(!schedule.due(cadence, at(14, hour, 0)), "hour {hour}");
        }
    }

    #[test]
    fn lessons_commit_runs_once_a_day_from_half_past_one_utc() {
        let mut schedule = Schedule::default();
        let cadence = &LESSONS_COMMIT.cadence;
        assert!(!schedule.due(cadence, at(14, 1, 29)));
        assert!(schedule.due(cadence, at(14, 1, 30)));
        assert!(schedule.due(cadence, at(14, 3, 59)));
        assert!(!schedule.due(cadence, at(14, 4, 30)));
        assert!(!schedule.due(cadence, at(14, 13, 30)));
        schedule.begin(cadence, at(14, 1, 31));
        schedule.finish(at(14, 1, 32), true);
        assert!(!schedule.due(cadence, at(14, 2, 30)));
        assert!(!schedule.due(cadence, at(14, 3, 30)));
        assert!(schedule.due(cadence, at(15, 1, 30)));
    }

    #[test]
    fn lessons_commit_next_run_is_the_coming_half_past_one() {
        assert_eq!(
            LESSONS_COMMIT.cadence.next_due_after(at(14, 1, 30)),
            Some(at(15, 1, 30))
        );
        assert_eq!(
            LESSONS_COMMIT.cadence.next_due_after(at(14, 0, 10)),
            Some(at(14, 1, 30))
        );
    }

    #[test]
    fn usage_audit_is_due_only_on_sunday_morning() {
        let schedule = Schedule::default();
        let cadence = &USAGE_AUDIT.cadence;
        assert!(!schedule.due(cadence, at(11, 5, 59)));
        assert!(schedule.due(cadence, at(11, 6, 0)));
        assert!(schedule.due(cadence, at(11, 10, 59)));
        assert!(!schedule.due(cadence, at(11, 11, 0)));
        for day in [12, 13, 14, 15, 16, 17] {
            assert!(!schedule.due(cadence, at(day, 6, 0)), "day {day}");
        }
    }

    #[test]
    fn usage_audit_runs_once_a_week_and_again_the_next_sunday() {
        let mut schedule = Schedule::default();
        let cadence = &USAGE_AUDIT.cadence;
        schedule.begin(cadence, at(11, 6, 0));
        schedule.finish(at(11, 6, 5), true);
        assert!(!schedule.due(cadence, at(11, 8, 0)));
        assert!(schedule.due(cadence, at(18, 6, 0)));
        assert_eq!(cadence.next_due_after(at(11, 6, 5)), Some(at(18, 6, 0)));
        assert_eq!(cadence.next_due_after(at(14, 12, 0)), Some(at(18, 6, 0)));
    }

    #[test]
    fn next_due_after_is_the_earliest_opening_after_now() {
        assert_eq!(
            PROMOTE.cadence.next_due_after(at(14, 12, 0)),
            Some(at(14, 20, 30))
        );
        assert_eq!(
            PROMOTE.cadence.next_due_after(at(14, 23, 45)),
            Some(at(15, 0, 30))
        );
        assert_eq!(
            AUDIT.cadence.next_due_after(at(14, 9, 0)),
            Some(at(19, 1, 0))
        );
        assert_eq!(
            CARGO_SWEEP.cadence.next_due_after(at(14, 18, 0)),
            Some(at(15, 0, 0))
        );
    }

    #[test]
    fn next_run_is_now_while_the_window_is_open_and_the_period_unattempted() {
        let schedule = Schedule::default();
        assert_eq!(
            schedule.next_run_at(&AUDIT.cadence, at(12, 4, 0)),
            Some(at(12, 4, 0))
        );
    }

    #[test]
    fn next_run_moves_on_once_the_period_ran_and_waits_out_a_backoff() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 21, 30));
        schedule.finish(at(14, 21, 40), false);
        assert_eq!(
            schedule.next_run_at(&PROMOTE.cadence, at(14, 21, 41)),
            Some(at(14, 22, 30))
        );
        schedule.begin(&PROMOTE.cadence, at(14, 22, 30));
        schedule.finish(at(14, 22, 40), false);
        assert_eq!(schedule.retry_after(), Some(at(14, 23, 40)));
        assert_eq!(
            schedule.next_run_at(&PROMOTE.cadence, at(14, 22, 41)),
            Some(at(14, 23, 40))
        );
    }

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .single()
            .unwrap_or_default()
    }

    #[test]
    fn promote_is_due_inside_every_window_hour_from_half_past() {
        let schedule = Schedule::default();
        for hour in [20, 21, 22, 23, 0, 1, 2, 3] {
            assert!(
                schedule.due(&PROMOTE.cadence, at(14, hour, 30)),
                "hour {hour} at :30"
            );
            assert!(
                schedule.due(&PROMOTE.cadence, at(14, hour, 59)),
                "hour {hour} at :59"
            );
        }
    }

    #[test]
    fn promote_is_not_due_before_half_past() {
        let schedule = Schedule::default();
        for hour in [20, 23, 0, 3] {
            assert!(
                !schedule.due(&PROMOTE.cadence, at(14, hour, 0)),
                "hour {hour} at :00"
            );
            assert!(
                !schedule.due(&PROMOTE.cadence, at(14, hour, 29)),
                "hour {hour} at :29"
            );
        }
    }

    #[test]
    fn promote_is_not_due_outside_the_window_hours() {
        let schedule = Schedule::default();
        for hour in [4, 5, 9, 12, 15, 19] {
            assert!(
                !schedule.due(&PROMOTE.cadence, at(14, hour, 30)),
                "hour {hour}"
            );
            assert!(
                !schedule.due(&PROMOTE.cadence, at(14, hour, 45)),
                "hour {hour}"
            );
        }
    }

    #[test]
    fn an_hour_that_already_ran_is_skipped_until_the_next_one() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 21, 31));
        assert!(!schedule.due(&PROMOTE.cadence, at(14, 21, 32)));
        assert!(!schedule.due(&PROMOTE.cadence, at(14, 21, 59)));
        schedule.finish(at(14, 21, 40), true);
        assert!(!schedule.due(&PROMOTE.cadence, at(14, 21, 45)));
        assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
    }

    #[test]
    fn the_same_clock_hour_on_the_next_day_runs_again() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 22, 30));
        schedule.finish(at(14, 22, 31), true);
        assert!(schedule.due(&PROMOTE.cadence, at(15, 22, 30)));
    }

    #[test]
    fn a_run_that_started_late_in_an_hour_does_not_block_the_next_hour() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 21, 58));
        schedule.finish(at(14, 22, 10), true);
        assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
    }

    #[test]
    fn audit_is_due_only_on_monday_morning() {
        let schedule = Schedule::default();
        assert!(schedule.due(&AUDIT.cadence, at(12, 1, 0)));
        assert!(schedule.due(&AUDIT.cadence, at(12, 6, 59)));
        assert!(!schedule.due(&AUDIT.cadence, at(12, 0, 59)));
        assert!(!schedule.due(&AUDIT.cadence, at(12, 7, 0)));
        assert!(!schedule.due(&AUDIT.cadence, at(12, 22, 30)));
        for day in [13, 14, 15, 16, 17, 18] {
            assert!(
                !schedule.due(&AUDIT.cadence, at(day, 1, 0)),
                "day {day} is not a Monday"
            );
        }
    }

    #[test]
    fn audit_runs_once_a_week_and_again_the_next_monday() {
        let mut schedule = Schedule::default();
        schedule.begin(&AUDIT.cadence, at(12, 1, 0));
        schedule.finish(at(12, 1, 20), true);
        assert!(!schedule.due(&AUDIT.cadence, at(12, 2, 0)));
        assert!(!schedule.due(&AUDIT.cadence, at(12, 6, 30)));
        assert!(schedule.due(&AUDIT.cadence, at(19, 1, 0)));
    }

    #[test]
    fn audit_catches_up_when_the_proxy_comes_back_later_that_morning() {
        let schedule = Schedule::default();
        assert!(schedule.due(&AUDIT.cadence, at(12, 4, 15)));
    }

    #[test]
    fn backoff_doubles_from_half_an_hour_and_stops_at_four_hours() {
        assert_eq!(backoff_after(1), Duration::minutes(30));
        assert_eq!(backoff_after(2), Duration::hours(1));
        assert_eq!(backoff_after(3), Duration::hours(2));
        assert_eq!(backoff_after(4), Duration::hours(4));
        assert_eq!(backoff_after(5), Duration::hours(4));
        assert_eq!(backoff_after(u32::MAX), Duration::hours(4));
    }

    #[test]
    fn consecutive_failures_hold_the_next_attempt_back() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 20, 30));
        schedule.finish(at(14, 20, 31), false);
        schedule.begin(&PROMOTE.cadence, at(14, 21, 30));
        schedule.finish(at(14, 21, 31), false);
        assert_eq!(schedule.consecutive_failures(), 2);
        assert_eq!(schedule.retry_after(), Some(at(14, 22, 31)));
        assert!(!schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
        assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 31)));
    }

    #[test]
    fn a_success_clears_the_failures_and_the_hold() {
        let mut schedule = Schedule::default();
        schedule.begin(&PROMOTE.cadence, at(14, 20, 30));
        schedule.finish(at(14, 20, 31), false);
        schedule.begin(&PROMOTE.cadence, at(14, 21, 30));
        schedule.finish(at(14, 21, 40), true);
        assert_eq!(schedule.consecutive_failures(), 0);
        assert_eq!(schedule.retry_after(), None);
        assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
    }
}
