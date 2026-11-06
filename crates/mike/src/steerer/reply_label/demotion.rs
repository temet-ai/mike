use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tokio::time::{MissedTickBehavior, interval};

use crate::change_log::{ChangeSource, ChangeVia};
use crate::config::AppState;
use crate::logged::ok_or_warn;
use crate::steerer::guard::promotion::wilson::wilson_bounds;
use crate::steerer::service::SteeringService;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_metrics::{AlarmKind, Firing};
use crate::store::persistence::steering::SteererLabelWindow;

pub(crate) const MIN_LABELS: i64 = 10;
pub(crate) const PRECISION_BAR: f64 = 0.6;
const WINDOW_HOURS: i64 = 72;
const CHECK_EVERY: Duration = Duration::from_secs(600);

pub(crate) fn should_switch_off(right: i64, wrong: i64) -> bool {
    let n = right + wrong;
    n >= MIN_LABELS
        && wilson_bounds(right, n)
            .high
            .is_some_and(|upper| upper < PRECISION_BAR)
}

fn evidence(w: &SteererLabelWindow) -> serde_json::Value {
    json!({
        "rule": "reply_label_precision",
        "right": w.right,
        "wrong": w.wrong,
        "window_start": w.window_start,
        "bar": PRECISION_BAR,
        "min_labels": MIN_LABELS,
    })
}

fn log_switched_off(w: &SteererLabelWindow) {
    tracing::warn!(steerer = %w.name, right = w.right, wrong = w.wrong, "steerer demotion: switched off, most labelled notes were wrong");
}

fn log_failed(w: &SteererLabelWindow, error: &dyn std::fmt::Display) {
    tracing::warn!(steerer = %w.name, %error, "steerer demotion: switch-off failed");
}

async fn raise(persist: &PersistHandle, w: &SteererLabelWindow) {
    let firing = Firing {
        kind: AlarmKind::SteerQuestionDemoted,
        subject: format!("steerer/{}", w.name),
        detail: evidence(w),
    };
    ok_or_warn(
        persist.apply_alarm_pass(&[], &[firing], i32::MAX).await,
        "steerer_demotion.alarm",
    );
}

async fn switch_off(persist: &PersistHandle, svc: &SteeringService, w: &SteererLabelWindow) {
    let source = ChangeSource::new(ChangeVia::Mike)
        .with_detail("action", json!("auto_disable"))
        .with_detail("evidence", evidence(w));
    match svc
        .set_enabled_preserving_config(w.steerer_id, false, &source)
        .await
    {
        Ok(()) => {
            log_switched_off(w);
            raise(persist, w).await;
        }
        Err(error) => log_failed(w, &error),
    }
}

pub(crate) async fn run_steerer_demotion(persist: &PersistHandle, svc: &SteeringService) {
    let floor = Utc::now() - chrono::Duration::hours(WINDOW_HOURS);
    let Some(windows) = ok_or_warn(
        persist.steerer_label_windows(floor).await,
        "steerer_demotion.windows",
    ) else {
        return;
    };
    for w in windows
        .iter()
        .filter(|w| should_switch_off(w.right, w.wrong))
    {
        switch_off(persist, svc, w).await;
    }
}

pub(crate) fn spawn_steerer_demotion(state: &AppState) {
    let (Some(persist), Some(svc)) = (state.persist().cloned(), state.steerer_service().cloned())
    else {
        tracing::debug!("steerer demotion: no persistence or steering service — not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        let mut ticker = interval(CHECK_EVERY);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => run_steerer_demotion(&persist, &svc).await,
            }
        }
    });
}
