use std::time::Instant;

use super::names::HISTOGRAM_DB_QUERY_DURATION_MS;

pub(crate) const OUTCOME_OK: &str = "ok";
pub(crate) const OUTCOME_ERR: &str = "err";
pub(crate) const OUTCOME_DROPPED: &str = "dropped_before_completion";

#[must_use = "DbTimer must outlive the DB call it measures; let-bind it for the body"]
pub(crate) struct DbTimer {
    method: &'static str,
    start: Instant,
    outcome: &'static str,
}

impl DbTimer {
    pub(crate) fn new(method: &'static str) -> Self {
        Self {
            method,
            start: Instant::now(),
            outcome: OUTCOME_DROPPED,
        }
    }

    pub(crate) fn observe_result<T, E>(&mut self, result: &Result<T, E>) {
        self.outcome = match (self.outcome, result.is_err()) {
            (_, true) | (OUTCOME_ERR, false) => OUTCOME_ERR,
            (_, false) => OUTCOME_OK,
        };
    }
}

impl Drop for DbTimer {
    fn drop(&mut self) {
        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        metrics::histogram!(
            HISTOGRAM_DB_QUERY_DURATION_MS,
            "method" => self.method,
            "outcome" => self.outcome,
        )
        .record(elapsed_ms);
    }
}

pub(crate) async fn time_db<T, E, F>(method: &'static str, fut: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    let mut timer = DbTimer::new(method);
    let result = fut.await;
    timer.observe_result(&result);
    result
}

#[cfg(test)]
#[path = "db_timer_cancel_tests.rs"]
mod cancel_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_timer_before_any_result_is_dropped_before_completion() {
        let timer = DbTimer::new("test_method");
        assert_eq!(timer.outcome, OUTCOME_DROPPED);
        assert_eq!(timer.method, "test_method");
    }

    #[test]
    fn db_timer_observe_result_marks_err_for_err_variant() {
        let mut timer = DbTimer::new("test_method");
        let r: Result<(), &'static str> = Err("boom");
        timer.observe_result(&r);
        assert_eq!(timer.outcome, "err");
    }

    #[test]
    fn db_timer_observe_result_keeps_ok_for_ok_variant() {
        let mut timer = DbTimer::new("test_method");
        let r: Result<u32, &'static str> = Ok(42);
        timer.observe_result(&r);
        assert_eq!(timer.outcome, "ok");
    }

    #[test]
    fn db_timer_observe_result_does_not_revert_err_to_ok() {
        let mut timer = DbTimer::new("test_method");
        let bad: Result<(), &'static str> = Err("fail");
        timer.observe_result(&bad);
        let good: Result<(), &'static str> = Ok(());
        timer.observe_result(&good);
        assert_eq!(timer.outcome, "err");
    }

    #[tokio::test]
    async fn time_db_returns_inner_result_unchanged() {
        let ok: Result<u32, &'static str> = time_db("t", async { Ok(7) }).await;
        assert_eq!(ok, Ok(7));

        let err: Result<u32, &'static str> = time_db("t", async { Err("boom") }).await;
        assert_eq!(err, Err("boom"));
    }
}
