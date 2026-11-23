use super::guard::Guard;
use super::health;
use super::items::{Env, process_items};
use super::llm::{ChainLlm, ConsolidationLlm, Embedder, ServiceEmbedder};
use super::measure::record_measures;
use super::settings::Settings;
use super::subjects::{Run, process_subjects};
use super::tally::Tally;
use super::undo::apply_proposed_on_machine;
use crate::base_state::BaseState;
use crate::feature_toggles::live_gate::wait_until_on;
use crate::llm::side_call::SideCallChain;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const LEASE_ROLE: &str = "knowledge_consolidation";
const JUDGE_TIMEOUT: Duration = Duration::from_secs(240);
const DEFAULT_JUDGE_MODEL: &str = "claude/sonnet";
const APPLY_BATCH: i64 = 50;
const TOGGLE_POLL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Idle,
    NotLeader,
    Skipped(&'static str),
    Ran(Box<Tally>),
}

pub async fn tick<L: ConsolidationLlm, E: Embedder>(
    persist: &PersistHandle,
    llm: &L,
    embedder: &E,
    settings: &Settings,
    guard: &mut Guard,
    machine: Option<Uuid>,
) -> Outcome {
    if !embedder.available() {
        health::failed("the embedding service is disabled, so statements cannot be embedded");
        return Outcome::Skipped("embedding disabled");
    }
    if guard.open(Instant::now()) {
        return Outcome::Skipped("circuit open");
    }
    if !persist.holds_worker_role(LEASE_ROLE, settings.interval).await {
        health::not_leader();
        return Outcome::NotLeader;
    }
    sweep_detached(persist, settings).await;
    let due = match persist.knowledge().consolidation_due().await {
        Ok(due) => due,
        Err(error) => {
            health::failed(&error.to_string());
            return Outcome::Skipped("database error");
        }
    };
    if due.items == 0 && due.subjects == 0 {
        health::idle();
        return Outcome::Idle;
    }
    run_pass(persist, llm, embedder, settings, guard, machine).await
}

async fn sweep_detached(persist: &PersistHandle, settings: &Settings) {
    let swept = persist
        .knowledge()
        .sweep_detached_statements(chrono::Utc::now() + settings.settle)
        .await;
    if let Err(error) = swept {
        tracing::warn!(%error, "knowledge consolidation: sweeping detached statements failed");
    }
}

async fn run_pass<L: ConsolidationLlm, E: Embedder>(
    persist: &PersistHandle,
    llm: &L,
    embedder: &E,
    settings: &Settings,
    guard: &mut Guard,
    machine: Option<Uuid>,
) -> Outcome {
    let run_id = match persist
        .lessons()
        .start_consolidation_run("knowledge", "schedule")
        .await
    {
        Ok(id) => id,
        Err(error) => {
            health::failed(&error.to_string());
            return Outcome::Skipped("could not open a run");
        }
    };
    let env = Env {
        persist,
        llm,
        embedder,
        settings,
    };
    let tally = work(&env, guard, Run { run_id, machine }).await;
    finish_run(persist, run_id, &tally).await;
    health::passed(&tally.summary(), tally.last_error.as_deref());
    Outcome::Ran(Box::new(tally))
}

async fn work<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    run: Run,
) -> Tally {
    let mut tally = Tally::default();
    if let Err(failure) = process_items(env, guard, &mut tally).await {
        tally.error(failure.message);
    }
    if let Err(failure) = process_subjects(env, guard, &mut tally, &run).await {
        tally.error(failure.message);
    }
    if tally.subjects_judged > 0 {
        measure_and_tidy(env, &mut tally, run.run_id).await;
    }
    tally
}

async fn measure_and_tidy<L, E>(env: &Env<'_, L, E>, tally: &mut Tally, run_id: i64) {
    if let Err(failure) = record_measures(env, tally, run_id).await {
        tally.error(failure.message);
    }
    if let Err(error) = env.persist.knowledge().drop_empty_knowledge_subjects().await {
        tracing::warn!(%error, "knowledge consolidation: could not drop empty subjects");
    }
}

async fn finish_run(persist: &PersistHandle, run_id: i64, tally: &Tally) {
    let status = match (&tally.last_error, tally.worked()) {
        (None, _) => "completed",
        (Some(_), true) => "partial",
        (Some(_), false) => "failed",
    };
    if let Err(error) = persist
        .lessons()
        .finish_consolidation_run(run_id, status, tally.last_error.as_deref())
        .await
    {
        tracing::warn!(%error, "knowledge consolidation: could not finish the run row");
    }
}

fn judge_client(base: &BaseState, model: &str) -> Option<OpenRouterClient> {
    let config = base.config();
    let client = OpenRouterClient::new(
        String::new(),
        model.to_owned(),
        Some(config.steerer.openrouter_base_url.clone()),
        Some(JUDGE_TIMEOUT),
    )
    .ok()?
    .with_attribution(LlmRole::JudgeWorker, base.llm_usage_sink.as_ref());
    crate::llm::key::effective_client(&client, &base.steerer_openrouter_key)
}

fn build_llm(base: &BaseState, settings: &Settings) -> Option<ChainLlm> {
    let models = base.settings_overlay.load().side_call_models.clone();
    let build = |model: &str| crate::roster_worker::build_client(base, model);
    let split_chains: Vec<SideCallChain> = models
        .iter()
        .filter_map(|model| build(model))
        .map(|client| SideCallChain::new(vec![client]))
        .collect();
    if split_chains.is_empty() {
        return None;
    }
    let judge_model = if settings.model.is_empty() {
        DEFAULT_JUDGE_MODEL
    } else {
        settings.model.as_str()
    };
    Some(ChainLlm {
        split_chains,
        check_chain: SideCallChain::from_models(&models, build),
        judge_client: judge_client(base, judge_model)?,
        judge_label: if settings.model.is_empty() {
            "standard tier".to_owned()
        } else {
            settings.model.clone()
        },
    })
}

type LlmKey = (String, Vec<String>);
type CachedLlm = Option<(LlmKey, Arc<ChainLlm>)>;

struct Worker {
    persist: PersistHandle,
    base: BaseState,
    embedder: ServiceEmbedder,
    guard: Guard,
    llm: CachedLlm,
    machine: Option<Uuid>,
}

impl Worker {
    fn llm(&mut self, settings: &Settings) -> Option<Arc<ChainLlm>> {
        let key = (
            settings.model.clone(),
            self.base.settings_overlay.load().side_call_models.clone(),
        );
        if let Some((built_for, llm)) = &self.llm
            && *built_for == key
        {
            return Some(llm.clone());
        }
        let llm = Arc::new(build_llm(&self.base, settings)?);
        self.llm = Some((key, llm.clone()));
        Some(llm)
    }

    async fn machine(&mut self) -> Option<Uuid> {
        if self.machine.is_none() {
            self.machine = crate::machine_identity::load_default()
                .await
                .ok()
                .map(|identity| identity.machine_uuid);
        }
        self.machine
    }

    async fn apply_edits(&self, settings: &Settings, machine: Option<Uuid>) {
        let (true, Some(machine)) = (settings.edit_files, machine) else {
            return;
        };
        let applied = apply_proposed_on_machine(&self.persist, machine, APPLY_BATCH).await;
        if applied > 0 {
            tracing::info!(applied, "knowledge consolidation: proposed file edits applied on this machine");
        }
    }

    async fn pass(&mut self, settings: &Settings) {
        let Some(llm) = self.llm(settings) else {
            health::failed("no model could be built for consolidation; check the side-call models and the consolidation model at /settings");
            return;
        };
        let machine = self.machine().await;
        self.apply_edits(settings, machine).await;
        let outcome = tick(
            &self.persist,
            llm.as_ref(),
            &self.embedder,
            settings,
            &mut self.guard,
            machine,
        )
        .await;
        if let Outcome::Ran(tally) = outcome {
            tracing::info!(summary = %tally.summary(), "knowledge consolidation: pass finished");
        }
    }
}

pub fn spawn_knowledge_consolidation(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("knowledge consolidation: no persistence, worker not spawned");
        return;
    };
    let worker = Worker {
        persist,
        base: state.base.clone(),
        embedder: ServiceEmbedder(state.embedding_service()),
        guard: Guard::new(),
        llm: None,
        machine: None,
    };
    let toggles = state.feature_toggle_overlay.clone();
    let shutdown = state.shutdown();
    state.tracker.spawn(run(worker, toggles, shutdown));
    tracing::info!("knowledge consolidation: worker spawned");
}

async fn run(
    mut worker: Worker,
    toggles: Arc<arc_swap::ArcSwap<crate::feature_toggles::FeatureToggles>>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    while wait_until_on(&toggles, |t| t.knowledge_consolidation, &shutdown).await {
        health::set_running(true);
        while toggles.load().knowledge_consolidation {
            let settings = Settings::load(&worker.persist).await;
            worker.pass(&settings).await;
            tokio::select! {
                biased;
                () = shutdown.cancelled() => {
                    health::set_running(false);
                    return;
                }
                () = tokio::time::sleep(settings.interval.max(TOGGLE_POLL)) => {}
            }
        }
        health::set_running(false);
    }
}
