use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::components::{
    SteererComponentInputs, SteererPrompt, session_judge_components, steerer_components,
    turn_judge_components,
};
use super::{BundleCandidate, BundleCause, BundleIds, BundleRole};
use crate::compose_prompt::decision::Composition;
use crate::config::SettingsWriteGuard;
use crate::config::session_judge::SessionJudgeConfig;
use crate::settings::SettingsOverlay;
use crate::steerer::config::JudgeConfig;
use crate::steerer::refocus::{block2_or_default, build_verdict_system_prompt};
use crate::steerer::registry::{Steerer, SteererRegistry};
use crate::store::persistence::eval_bundles::{BuildOrigin, EnsuredBundle, NewBundleRow};
use crate::store::persistence::{PersistError, PersistHandle};

const RETRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct BundleStaticInputs {
    pub(crate) judge: JudgeConfig,
    pub(crate) session_judge: SessionJudgeConfig,
    pub(crate) fleet_default_model: String,
    pub(crate) smoke_run: bool,
}

pub struct BundleResolver {
    persist: PersistHandle,
    inputs: BundleStaticInputs,
    registry: Arc<ArcSwap<SteererRegistry>>,
}

fn steerer_prompt(
    steerer: &Steerer,
    compositions: &HashMap<i64, Vec<Composition>>,
) -> SteererPrompt {
    if let Some(text) = steerer
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return SteererPrompt {
            text: text.to_owned(),
            source: "row".to_owned(),
        };
    }
    let empty = Vec::new();
    let list = compositions.get(&steerer.agent_id).unwrap_or(&empty);
    let (text, origin) = block2_or_default(list, build_verdict_system_prompt);
    let source = origin.map_or_else(
        || "compiled".to_owned(),
        |(id, version)| format!("composition:{id}@v{version}"),
    );
    SteererPrompt { text, source }
}

fn into_ids(rows: Vec<EnsuredBundle>) -> BundleIds {
    let mut ids = BundleIds::default();
    for row in rows {
        match row.role.as_str() {
            "turn_judge" => ids.turn_judge = Some(row.id),
            "session_judge" => ids.session_judge = Some(row.id),
            "steerer" => {
                if let Ok(steerer_id) = row.subject_key.parse::<i64>() {
                    ids.steerers.insert(steerer_id, row.id);
                }
            }
            _ => {}
        }
    }
    ids
}

impl BundleResolver {
    pub(crate) fn new(
        persist: PersistHandle,
        inputs: BundleStaticInputs,
        registry: Arc<ArcSwap<SteererRegistry>>,
    ) -> Self {
        Self {
            persist,
            inputs,
            registry,
        }
    }

    async fn candidates(
        &self,
        overlay: &SettingsOverlay,
    ) -> Result<Vec<BundleCandidate>, PersistError> {
        let mut candidates = Vec::new();
        if overlay.turn_judge_live_bundle_id <= 0 {
            candidates.push(BundleCandidate::new(
                BundleRole::TurnJudge,
                String::new(),
                turn_judge_components(
                    overlay,
                    &self.inputs.judge,
                    &self.inputs.fleet_default_model,
                ),
            ));
        }
        if overlay.session_judge_live_bundle_id <= 0 {
            candidates.push(BundleCandidate::new(
                BundleRole::SessionJudge,
                String::new(),
                session_judge_components(overlay, &self.inputs.session_judge),
            ));
        }
        let registry = self.registry.load();
        let agent_ids: Vec<i64> = registry
            .steerers
            .iter()
            .filter(|s| s.prompt.as_deref().is_none_or(|p| p.trim().is_empty()))
            .map(|s| s.agent_id)
            .collect();
        let compositions = self
            .persist
            .list_compositions_for_agents(&agent_ids)
            .await?;
        for steerer in &registry.steerers {
            let prompt = steerer_prompt(steerer, &compositions);
            let components = steerer_components(&SteererComponentInputs {
                steerer,
                prompt: &prompt,
                fleet_default_model: &self.inputs.fleet_default_model,
                overlay,
                extras: None,
            });
            candidates.push(BundleCandidate::new(
                BundleRole::Steerer,
                steerer.id.to_string(),
                components,
            ));
        }
        Ok(candidates)
    }

    pub(crate) async fn try_resolve(
        &self,
        overlay: &SettingsOverlay,
        cause: &BundleCause,
    ) -> Result<BundleIds, PersistError> {
        let candidates = self.candidates(overlay).await?;
        let rows: Vec<NewBundleRow> = candidates
            .into_iter()
            .map(|c| NewBundleRow {
                role: c.role.code().to_owned(),
                subject_key: c.subject_key,
                components: c.components,
                content_sha256: c.content_sha256,
            })
            .collect();
        let build = crate::change_log::current_build();
        let ensured = self
            .persist
            .ensure_bundles(
                &rows,
                BuildOrigin {
                    sha: build.sha,
                    dirty: build.dirty,
                },
                &cause.to_value(),
            )
            .await?;
        let mut ids = into_ids(ensured);
        if overlay.turn_judge_live_bundle_id > 0 {
            ids.turn_judge = Some(overlay.turn_judge_live_bundle_id);
        }
        if overlay.session_judge_live_bundle_id > 0 {
            ids.session_judge = Some(overlay.session_judge_live_bundle_id);
        }
        Ok(ids)
    }
}

pub struct BundlePublisher {
    resolver: BundleResolver,
    smoke_run: bool,
    overlay: Arc<ArcSwap<SettingsOverlay>>,
    write_lock: Arc<Mutex<SettingsWriteGuard>>,
    last_failed: AtomicBool,
}

impl BundlePublisher {
    pub(crate) fn new(
        resolver: BundleResolver,
        overlay: Arc<ArcSwap<SettingsOverlay>>,
        write_lock: Arc<Mutex<SettingsWriteGuard>>,
    ) -> Self {
        let smoke_run = resolver.inputs.smoke_run;
        Self {
            resolver,
            smoke_run,
            overlay,
            write_lock,
            last_failed: AtomicBool::new(false),
        }
    }

    pub(crate) async fn resolve_ids(
        &self,
        overlay: &SettingsOverlay,
        cause: BundleCause,
    ) -> BundleIds {
        if self.smoke_run {
            return BundleIds::default();
        }
        match self.resolver.try_resolve(overlay, &cause).await {
            Ok(ids) => {
                self.last_failed.store(false, Ordering::Relaxed);
                ids
            }
            Err(e) => {
                tracing::warn!(error = %e, trigger = cause.trigger, "bundle resolver failed");
                self.last_failed.store(true, Ordering::Relaxed);
                BundleIds::default()
            }
        }
    }

    pub(crate) async fn republish(&self, cause: BundleCause) {
        let _guard = self.write_lock.lock().await;
        let current = self.overlay.load_full();
        let mut next = (*current).clone();
        next.bundles = Arc::new(self.resolve_ids(&next, cause).await);
        self.overlay.store(Arc::new(next));
    }

    pub(crate) fn needs_retry(&self) -> bool {
        self.last_failed.load(Ordering::Relaxed)
    }

    pub(crate) fn spawn_retry(
        self: &Arc<Self>,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) {
        let publisher = Arc::clone(self);
        tracker.spawn(async move {
            let mut ticker = tokio::time::interval(RETRY_INTERVAL);
            ticker.tick().await;
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = ticker.tick() => {
                        if publisher.needs_retry() {
                            publisher.republish(BundleCause::retry()).await;
                        }
                    }
                }
            }
        });
    }
}
