use crate::store::persistence::eval_repo::EvalPersist as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::circuit_breaker::CircuitBreaker;
use crate::dossier::view::types::AsOf;
use crate::feature_toggles::FeatureToggles;
use crate::llm::key::OpenRouterKeySlot;
use crate::llm::limiter::Reservation;
use crate::llm::{LlmCallContext, LlmRole, OpenRouterClient};
use crate::logged::ok_or_warn;
use crate::settings::SettingsOverlay;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_end::{ShadowSelection, V3Candidate};

use super::config::SessionJudgeV3Config;
use super::live::verdict_row;
use super::rubric::RUBRIC_VERSION;
use super::run::{SessionJudgeFailure, SessionSubject, judge_session_v3};

const POOL_SIZE: i64 = 200;

pub fn spawn_session_end_shadow(state: &AppState) {
    let Some(persist) = state.persist.clone() else {
        log_not_spawning_no_persistence();
        return;
    };

    let timeout_secs = state
        .config()
        .sections
        .get::<crate::features_settings::SessionJudgeConfig>()
        .request_timeout_secs;
    let llm = match OpenRouterClient::new(
        String::new(),
        String::new(),
        None,
        Some(Duration::from_secs(timeout_secs)),
    ) {
        Ok(client) => client,
        Err(e) => {
            log_client_build_failed(&e);
            return;
        }
    };
    let llm = llm.with_attribution(LlmRole::Eval, state.llm_usage_sink.as_ref());

    let cfg = state
        .config()
        .sections
        .get::<crate::features_settings::SessionJudgeConfig>();

    let deps = ShadowWorkerDeps {
        persist,
        llm,
        key_slot: state.steerer_openrouter_key.clone(),
        toggles: state.feature_toggle_overlay.clone(),
        settings: state.settings_overlay.clone(),
        live_config: state.live_config.clone(),
        breaker_threshold: cfg.circuit_breaker_threshold,
        breaker_cooldown_base_secs: cfg.circuit_breaker_cooldown_base_secs,
        breaker_cooldown_max_secs: cfg.circuit_breaker_cooldown_max_secs,
        shutdown: state.shutdown(),
    };

    state.tracker.spawn(async move {
        run_shadow_loop(deps).await;
    });

    log_spawned();
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("session_end_shadow: persistence not wired — not spawning");
}

fn log_client_build_failed(e: &crate::llm::LlmError) {
    tracing::warn!(error = %e, "session_end_shadow: failed to build LLM client — not spawning");
}

fn log_spawned() {
    tracing::info!("session_end_shadow: worker spawned");
}

fn log_shutdown_signalled() {
    tracing::debug!("session_end_shadow: shutdown signalled, exiting");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateProblem {
    NotConfigured,
    SameAsLive,
}

fn candidate_problem(shadow_bundle_id: i64, live_bundle_id: i64) -> Option<CandidateProblem> {
    if shadow_bundle_id <= 0 {
        Some(CandidateProblem::NotConfigured)
    } else if shadow_bundle_id == live_bundle_id {
        Some(CandidateProblem::SameAsLive)
    } else {
        None
    }
}

fn log_no_candidate() {
    tracing::warn!(
        "session_end_shadow is on but session_end_shadow.bundle_id is 0: no candidate to judge with"
    );
}

fn log_same_as_live(bundle_id: i64) {
    tracing::warn!(
        bundle_id,
        "session_end_shadow.bundle_id is the live session-judge bundle: nothing to compare"
    );
}

fn log_client_not_built() {
    tracing::warn!(
        "session_end_shadow: the judge LLM client could not be built — skipping this tick"
    );
}

struct ShadowWorkerDeps {
    persist: PersistHandle,
    llm: OpenRouterClient,
    key_slot: OpenRouterKeySlot,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    settings: Arc<ArcSwap<SettingsOverlay>>,
    live_config: Arc<crate::config::live::LiveConfig>,
    breaker_threshold: u32,
    breaker_cooldown_base_secs: u64,
    breaker_cooldown_max_secs: u64,
    shutdown: CancellationToken,
}

impl ShadowWorkerDeps {
    fn poll_interval(&self) -> Duration {
        let secs = self
            .settings
            .load()
            .session_end_shadow_interval_secs
            .max(60);
        Duration::from_secs(u64::try_from(secs).unwrap_or(300))
    }

    fn request_timeout(&self) -> Duration {
        let config = self
            .live_config
            .load()
            .sections
            .get::<crate::features_settings::SessionJudgeConfig>();
        Duration::from_secs(config.request_timeout_secs)
    }
}

async fn run_shadow_loop(deps: ShadowWorkerDeps) {
    let mut breaker = CircuitBreaker::new(
        deps.breaker_threshold,
        Duration::from_secs(deps.breaker_cooldown_base_secs),
        Duration::from_secs(deps.breaker_cooldown_max_secs),
    );

    let mut wait = Duration::ZERO;
    loop {
        tokio::select! {
            biased;

            _ = deps.shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = tokio::time::sleep(wait) => {
                wait = deps.poll_interval();
                if deps.toggles.load().session_end_shadow {
                    shadow_tick_if_leader(&deps, wait, &mut breaker).await;
                }
            }
        }
    }
}

async fn shadow_tick_if_leader(
    deps: &ShadowWorkerDeps,
    poll: Duration,
    breaker: &mut CircuitBreaker,
) {
    if !deps.persist.holds_llm_role(LlmRole::Eval, poll).await {
        return;
    }
    let ctx = ShadowTickDeps {
        persist: deps.persist.clone(),
        llm: deps.llm.with_timeout(deps.request_timeout()),
        key_slot: deps.key_slot.clone(),
    };
    let settings = deps.settings.load();
    tick(&ctx, &settings, breaker).await;
}

struct ShadowTickDeps {
    persist: PersistHandle,
    llm: OpenRouterClient,
    key_slot: OpenRouterKeySlot,
}

async fn tick(
    deps: &ShadowTickDeps,
    settings: &SettingsOverlay,
    breaker: &mut CircuitBreaker,
) -> usize {
    let bundle_id = settings.session_end_shadow_bundle_id;
    match candidate_problem(bundle_id, settings.session_judge_live_bundle_id) {
        Some(CandidateProblem::NotConfigured) => {
            log_no_candidate();
            return 0;
        }
        Some(CandidateProblem::SameAsLive) => {
            log_same_as_live(bundle_id);
            return 0;
        }
        None => {}
    }
    if breaker.should_skip(Instant::now()) {
        return 0;
    }
    let Some(llm) = crate::llm::key::keyed_client(&deps.llm, &deps.key_slot) else {
        log_client_not_built();
        return 0;
    };
    let probing = breaker.is_probing(Instant::now());
    let batch = if probing {
        1
    } else {
        settings.session_end_shadow_batch_size.max(1)
    };
    let Some(reservation) = reserve_batch(&llm, batch) else {
        return 0;
    };
    let llm = llm.with_reservation(&reservation);
    let poll = ShadowPoll {
        persist: &deps.persist,
        llm: &llm,
        bundle_id,
        lookback_hours: settings.session_end_shadow_lookback_hours,
        max_attempts_per_day: settings
            .session_judge_max_attempts_per_day
            .unwrap_or(crate::store::persistence::judge::DEFAULT_MAX_ATTEMPTS_PER_DAY),
        batch: i64::from(reservation.granted()),
        sample_rate: settings.session_end_shadow_sample_rate.clamp(0.0, 1.0),
    };
    poll.run(breaker).await
}

fn reserve_batch(llm: &OpenRouterClient, batch: i64) -> Option<Reservation> {
    let budget = llm.budget();
    let sized = batch.min(i64::from(budget.calls_now));
    if sized <= 0 {
        return None;
    }
    let reservation = llm.reserve(u32::try_from(sized).unwrap_or(u32::MAX));
    (reservation.granted() > 0).then_some(reservation)
}

async fn load_shadow_config(
    persist: &PersistHandle,
    bundle_id: i64,
) -> Option<SessionJudgeV3Config> {
    let row = ok_or_warn(
        persist.eval().get_bundle(bundle_id).await,
        "session_end_shadow.read_bundle",
    )??;
    ok_or_warn(
        SessionJudgeV3Config::from_bundle_components(&row.components),
        "session_end_shadow.bundle_components",
    )
}

struct ShadowPoll<'a> {
    persist: &'a PersistHandle,
    llm: &'a OpenRouterClient,
    bundle_id: i64,
    lookback_hours: i64,
    max_attempts_per_day: i64,
    batch: i64,
    sample_rate: f64,
}

impl ShadowPoll<'_> {
    async fn one(
        &self,
        cfg: &SessionJudgeV3Config,
        c: &V3Candidate,
        breaker: &mut CircuitBreaker,
    ) -> bool {
        let llm = self.llm.with_call_context(LlmCallContext {
            session_id: Some(c.session_db_id),
            steerer_id: None,
            operator_turn_id: Some(c.end_turn_id),
        });
        let subject = SessionSubject {
            session_db_id: c.session_db_id,
            as_of: AsOf::knowledge(Utc::now()),
        };
        let shadow_key = format!("shadow:{}", cfg.model);
        match judge_session_v3(self.persist, &llm, cfg, &subject).await {
            Ok(judged) => {
                breaker.record(true, Instant::now());
                let row = verdict_row(
                    &judged,
                    cfg,
                    c.session_db_id,
                    "shadow",
                    Some(self.bundle_id),
                );
                let stored = ok_or_warn(
                    self.persist.eval().insert_session_verdict_v3(&row).await,
                    "session_end_shadow.store_verdict",
                );
                self.record(c.session_db_id, &shadow_key, stored.is_none())
                    .await;
                true
            }
            Err(SessionJudgeFailure::Refused) => false,
            Err(e) => {
                breaker.record(false, Instant::now());
                tracing::warn!(session_id = %c.session_uuid, error = %e, "session_end_shadow: judging failed");
                self.record(c.session_db_id, &shadow_key, true).await;
                !breaker.should_skip(Instant::now())
            }
        }
    }

    async fn record(&self, session_db_id: i64, shadow_key: &str, failed: bool) {
        ok_or_warn(
            self.persist
                .eval()
                .record_judge_attempt(
                    session_db_id,
                    shadow_key,
                    RUBRIC_VERSION,
                    Utc::now(),
                    failed,
                )
                .await,
            "session_end_shadow.record_attempt",
        );
    }

    async fn run(&self, breaker: &mut CircuitBreaker) -> usize {
        let Some(cfg) = load_shadow_config(self.persist, self.bundle_id).await else {
            return 0;
        };
        let shadow_key = format!("shadow:{}", cfg.model);
        let selection = ShadowSelection {
            judge_model: &shadow_key,
            bundle_id: self.bundle_id,
            lookback_hours: i32::try_from(self.lookback_hours).unwrap_or(72),
            max_attempts_per_day: self.max_attempts_per_day,
            limit: POOL_SIZE,
        };
        let Some(candidates) = ok_or_warn(
            self.persist
                .eval()
                .fetch_sessions_needing_shadow(&selection)
                .await,
            "session_end_shadow.selection",
        ) else {
            return 0;
        };
        let take = usize::try_from(self.batch).unwrap_or(0);
        let mut judged = 0;
        for c in candidates
            .iter()
            .filter(|c| should_sample(c.end_turn_id, self.sample_rate))
            .take(take)
        {
            if !self.one(&cfg, c, breaker).await {
                break;
            }
            judged += 1;
        }
        judged
    }
}

fn sample_point(end_turn_id: i64) -> f64 {
    let mut x = end_turn_id.unsigned_abs() ^ 0x9E37_79B9_7F4A_7C15;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x as f64) / (u64::MAX as f64)
}

pub fn should_sample(end_turn_id: i64, sample_rate: f64) -> bool {
    if sample_rate <= 0.0 {
        return false;
    }
    if sample_rate >= 1.0 {
        return true;
    }
    sample_point(end_turn_id) < sample_rate
}

#[cfg(test)]
mod tests {
    use super::{CandidateProblem, candidate_problem, sample_point, should_sample};

    #[test]
    fn candidate_problem_unset_bundle_is_not_configured() {
        assert_eq!(
            candidate_problem(0, 95),
            Some(CandidateProblem::NotConfigured)
        );
    }

    #[test]
    fn candidate_problem_live_bundle_is_same_as_live() {
        assert_eq!(
            candidate_problem(95, 95),
            Some(CandidateProblem::SameAsLive)
        );
    }

    #[test]
    fn candidate_problem_other_bundle_is_none() {
        assert_eq!(candidate_problem(96, 95), None);
    }

    #[test]
    fn zero_rate_never_samples() {
        for id in [1_i64, 2, 100, 999_999] {
            assert!(!should_sample(id, 0.0));
        }
    }

    #[test]
    fn full_rate_always_samples() {
        for id in [1_i64, 2, 100, 999_999] {
            assert!(should_sample(id, 1.0));
        }
    }

    #[test]
    fn sampling_is_deterministic_for_the_same_turn() {
        for id in [7_i64, 42, 31_337] {
            let first = should_sample(id, 0.37);
            for _ in 0..5 {
                assert_eq!(should_sample(id, 0.37), first);
            }
        }
    }

    #[test]
    fn a_higher_rate_only_ever_adds_ids_never_removes_them() {
        for id in 1_i64..2_000 {
            if should_sample(id, 0.2) {
                assert!(should_sample(id, 0.6), "id {id} dropped when the rate rose");
            }
            if should_sample(id, 0.6) {
                assert!(should_sample(id, 0.9), "id {id} dropped when the rate rose");
            }
        }
    }

    #[test]
    fn sample_rate_of_0_2_keeps_roughly_a_fifth_over_many_ids() {
        let n = 20_000;
        let kept = (1_i64..=n).filter(|id| should_sample(*id, 0.2)).count();
        let share =
            f64::from(u32::try_from(kept).unwrap_or(0)) / f64::from(u32::try_from(n).unwrap_or(1));
        assert!((0.15..=0.25).contains(&share), "share was {share}");
    }

    #[test]
    fn sample_point_stays_in_unit_range() {
        for id in [0_i64, 1, i64::MAX, 123_456_789] {
            let p = sample_point(id);
            assert!((0.0..1.0).contains(&p), "{id} -> {p}");
        }
    }
}
