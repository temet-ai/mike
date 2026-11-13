use std::time::Instant;

use super::names::HISTOGRAM_SEARCH_PHASE_DURATION_MS;

#[must_use = "PhaseTimer must outlive the work it measures; let-bind it for the body"]
pub struct PhaseTimer {
    phase: &'static str,
    start: Instant,
}

impl PhaseTimer {
    pub fn new(phase: &'static str) -> Self {
        Self {
            phase,
            start: Instant::now(),
        }
    }
}

impl Drop for PhaseTimer {
    fn drop(&mut self) {
        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        metrics::histogram!(HISTOGRAM_SEARCH_PHASE_DURATION_MS, "phase" => self.phase)
            .record(elapsed_ms);
    }
}

pub async fn time_phase<T, F>(phase: &'static str, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let _timer = PhaseTimer::new(phase);
    fut.await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn time_phase_returns_the_inner_value_unchanged() {
        let value = time_phase("test_phase", async { 7 }).await;
        assert_eq!(value, 7);
    }

    #[tokio::test]
    async fn time_phase_passes_a_result_through_unchanged() {
        let outcome: Result<u32, &str> = time_phase("test_phase", async { Err("boom") }).await;
        assert_eq!(outcome, Err("boom"));
    }
}
