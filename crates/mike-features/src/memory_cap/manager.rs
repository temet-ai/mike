use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval, timeout};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::sheddable::{ShedReport, Sheddable, priority_label};
use crate::config::memory_cap::CapManagerConfig;
use crate::state::AppState;

pub fn spawn_cap_manager(
    state: AppState,
    config: CapManagerConfig,
    tracker: &TaskTracker,
    cancel: CancellationToken,
) {
    tracker.spawn(async move {
        let registered = state.shed_registry().len();
        if config.rss_ceiling_bytes == 0 {
            tracing::info!(
                target: "mike::cap_manager",
                registered_shed_ables = registered,
                "cap manager spawned with rss_ceiling_bytes=0 — observation only, no shedding will occur",
            );
        } else {
            tracing::info!(
                target: "mike::cap_manager",
                rss_ceiling_bytes = config.rss_ceiling_bytes,
                high_watermark = config.high_watermark(),
                low_watermark = config.low_watermark(),
                interval_ms = config.interval.as_millis() as u64,
                registered_shed_ables = registered,
                "cap manager spawned",
            );
        }

        let mut ticker = interval(config.interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await;

        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    tracing::debug!(
                        target: "mike::cap_manager",
                        "cap manager cancelled",
                    );
                    break;
                }
                _ = ticker.tick() => {
                    run_one_tick(&state, &config).await;
                }
            }
        }
    });
}

async fn run_one_tick(state: &AppState, config: &CapManagerConfig) {
    if config.rss_ceiling_bytes == 0 {
        let _ = crate::diagnostics::rss::read_max_rss();
        return;
    }
    let Some(rss) = crate::diagnostics::rss::read_max_rss() else {
        tracing::warn!(
            target: "mike::cap_manager",
            "cap_manager: rss unavailable; this tick is a no-op",
        );
        return;
    };

    let high = config.high_watermark();
    let low = config.low_watermark();

    if rss <= low {
        clear_admission_throttle(state, rss, low);
        return;
    }

    if rss <= high {
        return;
    }

    run_shed_tick(state, rss, high, low).await;
}

fn clear_admission_throttle(state: &AppState, rss: u64, low: u64) {
    if !state.is_capture_admission_throttled() {
        return;
    }
    state.set_capture_admission_throttled(false);
    tracing::info!(
        target: "mike::cap_manager",
        rss = rss,
        low_watermark = low,
        "cap_manager: cleared capture admission throttle (rss back under low watermark)",
    );
}

async fn run_shed_tick(state: &AppState, rss: u64, high: u64, low: u64) {
    let target = rss.saturating_sub(low);
    let started = Instant::now();
    let (bytes_freed, tick_actions, critical_only_pressure) =
        shed_registered_paths(state, target).await;

    if (critical_only_pressure || rss.saturating_sub(bytes_freed) > high)
        && !state.is_capture_admission_throttled()
    {
        state.set_capture_admission_throttled(true);
        warn_capture_bypassed(rss, high, bytes_freed);
    }

    log_cap_manager_tick(
        state,
        &CapManagerTickSummary {
            rss,
            high_watermark: high,
            low_watermark: low,
            target_bytes: target,
            bytes_freed,
            actions: tick_actions,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    );
}

fn warn_capture_bypassed(rss: u64, high: u64, bytes_freed: u64) {
    tracing::warn!(
        target: "mike::cap_manager",
        rss,
        high_watermark = high,
        bytes_freed = bytes_freed,
        "cap_manager: capture bypassed — forwarding without inspection until rss recovers",
    );
}

struct CapManagerTickSummary {
    rss: u64,
    high_watermark: u64,
    low_watermark: u64,
    target_bytes: u64,
    bytes_freed: u64,
    actions: u64,
    elapsed_ms: u64,
}

fn log_cap_manager_tick(state: &AppState, summary: &CapManagerTickSummary) {
    tracing::info!(
        target: "mike::cap_manager",
        event = "memory.cap_manager_tick",
        rss = summary.rss,
        high_watermark = summary.high_watermark,
        low_watermark = summary.low_watermark,
        target_bytes = summary.target_bytes,
        bytes_freed = summary.bytes_freed,
        actions = summary.actions,
        admission_throttled = state.is_capture_admission_throttled(),
        elapsed_ms = summary.elapsed_ms,
        "cap manager tick",
    );
}

async fn shed_registered_paths(state: &AppState, target: u64) -> (u64, u64, bool) {
    let mut bytes_freed: u64 = 0;
    let mut tick_actions: u64 = 0;
    let mut critical_only_pressure = true;

    for sheddable in state.shed_registry().iter() {
        let _priority = sheddable.priority();
        critical_only_pressure = false;
        if bytes_freed >= target {
            break;
        }
        let remaining = target.saturating_sub(bytes_freed);
        let report = timeout(Duration::from_millis(500), sheddable.shed(remaining)).await;
        match report {
            Ok(Some(r)) => {
                bytes_freed = bytes_freed.saturating_add(r.bytes_freed);
                tick_actions = tick_actions.saturating_add(r.actions);
                log_shed_report(sheddable, &r);
                if !r.more_available {
                    continue;
                }
            }
            Ok(None) => {}
            Err(_) => {
                tracing::warn!(
                    target: "mike::cap_manager",
                    path = ?sheddable.shed_path(),
                    "cap_manager: shed() timed out at 500ms; moving to next subsystem",
                );
            }
        }
    }
    (bytes_freed, tick_actions, critical_only_pressure)
}

fn log_shed_report(sheddable: &Arc<dyn Sheddable>, r: &ShedReport) {
    tracing::info!(
        target: "mike::cap_manager",
        priority = priority_label(sheddable.priority()),
        path = ?sheddable.shed_path(),
        bytes_freed = r.bytes_freed,
        actions = r.actions,
        more_available = r.more_available,
        "cap_manager: shed report",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_disables_cap_manager() {
        let cfg = CapManagerConfig::default();
        assert_eq!(cfg.rss_ceiling_bytes, 0);
        assert_eq!(cfg.high_watermark(), 0);
        assert_eq!(cfg.low_watermark(), 0);
    }

    #[test]
    fn watermarks_are_percentage_of_ceiling() {
        let cfg = CapManagerConfig {
            interval: Duration::from_secs(1),
            rss_ceiling_bytes: 1_000_000_000,
            rss_high_watermark_pct: 90,
            rss_low_watermark_pct: 75,
        };
        assert_eq!(cfg.high_watermark(), 900_000_000);
        assert_eq!(cfg.low_watermark(), 750_000_000);
    }
}
