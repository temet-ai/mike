use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::analysis::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::circuit_breaker::CircuitBreaker;
use crate::config::AppState;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{EmbedderRow, SemanticWindow};

use super::builder::{BuildReport, Builder};
use super::config::SemanticConfig;
use super::drift_job::DriftRun;
use super::floors_job::FloorRun;
use super::tag_job::{group_weekly, tag_tick};
use super::{SemanticError, SemanticHandle, check};

const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_BASE: Duration = Duration::from_secs(300);
const BREAKER_MAX: Duration = Duration::from_secs(3_600);
const DIRECTIVE_BATCH: i64 = 64;

pub struct SemanticWorker {
    pub persist: PersistHandle,
    pub embedding: EmbeddingService,
    pub toggles: Arc<ArcSwap<FeatureToggles>>,
    pub handle: SemanticHandle,
}

#[derive(Default)]
pub struct LoopState {
    last_snapshot: Option<(String, String, usize)>,
    last_check: Option<DateTime<Utc>>,
    cursor: i64,
    breaker_was_open: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TickReport {
    pub status: String,
    pub build: BuildReport,
    pub directives: u64,
    pub tagged: u64,
    pub floors_written: usize,
    pub drift_days: usize,
}

fn snapshot_key(c: &EmbeddingConfig) -> (String, String, usize) {
    (c.model.clone(), c.url.clone(), c.chunk_max_chars)
}

fn older_than(t: Option<DateTime<Utc>>, hours: i64) -> bool {
    t.is_none_or(|t| Utc::now() - t > chrono::Duration::hours(hours))
}

pub(super) fn floors_due(
    has_floors: bool,
    last_computed: Option<DateTime<Utc>>,
    last_attempt: Option<DateTime<Utc>>,
) -> bool {
    let recompute_hours = if has_floors { 24 * 7 } else { 24 };
    older_than(last_computed, recompute_hours) && older_than(last_attempt, 24)
}

fn log_skipped(e: &SemanticError) {
    tracing::debug!(error = %e, "semantic: tick skipped, embedding is disabled");
}

fn log_failed(e: &SemanticError) {
    tracing::warn!(error = %e, "semantic: tick failed");
}

fn log_tick(result: &Result<TickReport, SemanticError>) {
    let Err(e) = result else { return };
    if e.is_embedding_disabled() {
        log_skipped(e);
    } else {
        log_failed(e);
    }
}

impl SemanticWorker {
    async fn read_config(&self) -> SemanticConfig {
        match self.persist.fetch_settings().await {
            Ok(rows) => SemanticConfig::from_rows(&rows),
            Err(e) => {
                tracing::warn!(error = %e, "semantic: settings read failed; using the last config");
                self.handle.config()
            }
        }
    }

    async fn embedder(
        &self,
        cfg: &SemanticConfig,
        state: &mut LoopState,
    ) -> Result<EmbedderRow, SemanticError> {
        let snapshot = self.embedding.current_config();
        let changed = state.last_snapshot.as_ref() != Some(&snapshot_key(&snapshot));
        let due = changed || state.breaker_was_open || older_than(state.last_check, 24);
        if !due && let Some(row) = self.persist.current_semantic_embedder().await? {
            return Ok(row);
        }
        let row =
            check::ensure_embedder(&self.persist, &self.embedding, cfg.allow_remote_embedding)
                .await?;
        state.last_snapshot = Some(snapshot_key(&snapshot));
        state.last_check = Some(Utc::now());
        state.breaker_was_open = false;
        Ok(row)
    }

    async fn floors_are_due(
        &self,
        embedder: &EmbedderRow,
        last_attempt: Option<DateTime<Utc>>,
    ) -> Result<bool, SemanticError> {
        let key = &embedder.embedder_key;
        let has_floors = self.persist.semantic_key_has_floors(key).await?;
        let last_computed = self.persist.semantic_floors_last_computed_at(key).await?;
        Ok(floors_due(has_floors, last_computed, last_attempt))
    }

    async fn periodic(
        &self,
        cfg: &SemanticConfig,
        embedder: &EmbedderRow,
        report: &mut TickReport,
    ) -> Result<(), SemanticError> {
        let status = self.handle.load().status.clone();
        let weekly_due = older_than(status.last_weekly_run, 24 * 7);
        if self
            .floors_are_due(embedder, status.last_floor_attempt)
            .await?
        {
            self.handle
                .update(|s| s.status.last_floor_attempt = Some(Utc::now()));
            let run = FloorRun {
                persist: &self.persist,
                embedder,
                config: cfg,
                seed: "floors-v1",
                execute: true,
            };
            report.floors_written = run.compute_all().await?.len();
        }
        report.tagged = tag_tick(&self.persist, cfg, embedder).await?.written;
        if older_than(status.last_daily_run, 24) {
            let run = DriftRun {
                persist: &self.persist,
                config: cfg,
                embedder,
                write: true,
            };
            let lines = run
                .day(Utc::now().date_naive() - chrono::Duration::days(1))
                .await?;
            run.raise_alarms(&lines).await?;
            report.drift_days = 1;
            self.handle
                .update(|s| s.status.last_daily_run = Some(Utc::now()));
        }
        if weekly_due {
            group_weekly(&self.persist, cfg, embedder, true).await?;
            self.handle
                .update(|s| s.status.last_weekly_run = Some(Utc::now()));
        }
        Ok(())
    }

    pub async fn tick(&self, state: &mut LoopState) -> Result<TickReport, SemanticError> {
        if !self.toggles.load().semantic_context {
            return Ok(TickReport {
                status: "disabled".to_owned(),
                ..TickReport::default()
            });
        }
        let cfg = self.read_config().await;
        self.handle.update(|s| s.config = cfg.clone());
        let embedder = self.embedder(&cfg, state).await?;
        self.handle
            .update(|s| s.status.embedder_key = Some(embedder.embedder_key.clone()));
        let builder = Builder {
            persist: &self.persist,
            embedding: &self.embedding,
            config: &cfg,
            embedder: &embedder,
        };
        let window = SemanticWindow {
            from: None,
            until: None,
            after_turn: state.cursor,
            limit: cfg.batch_turns,
        };
        let build = builder.build_batch(&window).await?;
        state.cursor = if build.turns == 0 { 0 } else { build.last_turn };
        let mut report = TickReport {
            status: if build.egress_blocked {
                "egress_blocked"
            } else {
                "ok"
            }
            .to_owned(),
            build,
            directives: builder.build_directives(DIRECTIVE_BATCH).await?,
            ..TickReport::default()
        };
        self.periodic(&cfg, &embedder, &mut report).await?;
        Ok(report)
    }

    fn record(&self, result: &Result<TickReport, SemanticError>, breaker_open: bool) {
        self.handle.update(|s| {
            s.status.last_tick_at = Some(Utc::now());
            s.status.breaker_open = breaker_open;
            match result {
                Ok(r) => {
                    s.status.last_status = Some(r.status.clone());
                    s.status.last_error = None;
                }
                Err(e) => {
                    s.status.last_status = Some(e.status_code().to_owned());
                    s.status.last_error = Some(e.to_string());
                }
            }
        });
    }

    async fn guarded_tick(&self, state: &mut LoopState, breaker: &mut CircuitBreaker) {
        let result = self.tick(state).await;
        let ok = !matches!(&result, Err(e) if e.trips_breaker());
        breaker.record(ok, Instant::now());
        self.record(&result, breaker.should_skip(Instant::now()));
        log_tick(&result);
    }

    async fn run(self, shutdown: CancellationToken) {
        let mut state = LoopState::default();
        let mut breaker = CircuitBreaker::new(BREAKER_THRESHOLD, BREAKER_BASE, BREAKER_MAX);
        loop {
            let interval = Duration::from_secs(self.handle.config().sweep_interval_secs.max(60));
            if breaker.should_skip(Instant::now()) {
                state.breaker_was_open = true;
            } else {
                self.guarded_tick(&mut state, &mut breaker).await;
            }
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => return,
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }
}

pub fn spawn_semantic_worker(state: &AppState) {
    let Some(persist) = state.persist.clone() else {
        tracing::info!("semantic: not spawning, no persistence");
        return;
    };
    let worker = SemanticWorker {
        persist,
        embedding: state.embedding_service(),
        toggles: state.feature_toggle_overlay.clone(),
        handle: state.semantic(),
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(worker.run(shutdown));
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_semantic_once(state: &AppState) -> anyhow::Result<TickReport> {
    let Some(persist) = state.persist.clone() else {
        anyhow::bail!("persistence is off");
    };
    let worker = SemanticWorker {
        persist,
        embedding: state.embedding_service(),
        toggles: state.feature_toggle_overlay.clone(),
        handle: state.semantic(),
    };
    Ok(worker.tick(&mut LoopState::default()).await?)
}
