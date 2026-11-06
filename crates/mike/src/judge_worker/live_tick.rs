use std::time::Duration;

use crate::circuit_breaker::CircuitBreaker;
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;
use crate::steerer::config::JudgeConfig;
use crate::store::persistence::PersistHandle;

use super::live_judge::{
    LiveJudge, announce_bundle, load_live_bundle, reproject_final, run_live_sweep,
};
use super::live_shared::LiveJudgeShared;

pub(crate) struct LiveTick<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) llm: &'a OpenRouterClient,
    pub(crate) shared: &'a LiveJudgeShared,
    pub(crate) overlay: &'a SettingsOverlay,
    pub(crate) judge_cfg: &'a JudgeConfig,
}

pub(crate) const QUEUE_RECONCILE_SECS: i64 = 3_600;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LiveTickReport {
    pub(crate) promoted: usize,
    pub(crate) rejudged: usize,
    pub(crate) defer: Option<Duration>,
}

pub(crate) async fn run_live_tick(t: LiveTick<'_>, breaker: &mut CircuitBreaker) -> LiveTickReport {
    let Some(bundle) = load_live_bundle(t.persist, t.overlay.turn_judge_live_bundle_id).await
    else {
        return LiveTickReport::default();
    };
    announce_bundle(t.persist, t.shared, &bundle).await;
    reconcile_queue(&t, bundle.id).await;
    let judge = LiveJudge {
        persist: t.persist,
        llm: t.llm,
        shared: t.shared,
        bundle: &bundle,
        threshold: t
            .overlay
            .judge_worker_confidence_threshold
            .unwrap_or(t.judge_cfg.confidence_threshold),
        session_judge_bundle_id: t.overlay.session_judge_live_bundle_id,
        backoff_base_secs: t.judge_cfg.backoff_base_secs,
        backoff_max_secs: t.judge_cfg.backoff_max_secs,
    };
    let batch = t
        .overlay
        .judge_worker_batch_size
        .unwrap_or(t.judge_cfg.batch_size);
    let window_days = t.overlay.turn_judge_live_window_days;
    let sweep = run_live_sweep(&judge, breaker, batch, window_days).await;
    let rejudged = if sweep.defer.is_none() {
        super::rejudge::drain(&judge, breaker).await
    } else {
        0
    };
    reproject_final(&judge, window_days).await;
    LiveTickReport {
        promoted: sweep.labeled,
        rejudged,
        defer: sweep.defer,
    }
}

async fn reconcile_queue(t: &LiveTick<'_>, bundle_id: i64) {
    let now_unix = chrono::Utc::now().timestamp();
    if !t
        .shared
        .queue_reconcile_due(bundle_id, now_unix, QUEUE_RECONCILE_SECS)
    {
        return;
    }
    match t.persist.reconcile_judge_queue(bundle_id).await {
        Ok(done) => {
            t.shared.note_queue_reconciled(bundle_id, now_unix);
            log_queue_reconciled(bundle_id, done.queued, done.dequeued);
        }
        Err(error) => log_queue_reconcile_failed(bundle_id, &error),
    }
}

fn log_queue_reconciled(bundle_id: i64, queued: i64, dequeued: i64) {
    tracing::info!(
        bundle_id,
        queued,
        dequeued,
        "live judge: queue column reconciled"
    );
}

fn log_queue_reconcile_failed(bundle_id: i64, error: &impl std::fmt::Display) {
    tracing::warn!(%error, bundle_id, "live judge: queue column reconcile failed");
}
