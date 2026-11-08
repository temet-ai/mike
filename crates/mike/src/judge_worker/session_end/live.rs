use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::dossier::view::types::AsOf;
use crate::judge_worker::session_poll::digest::outcome_code_to_id;
use crate::judge_worker::session_poll::verdict_write::{
    refresh_vector_post_verdict, write_knowledge, write_overview,
};
use crate::llm::{LlmCallContext, OpenRouterClient};
use crate::logged::ok_or_warn;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::operator_turns::projection::{ProjectionMode, ProjectionParams};
use crate::store::persistence::session_end::{SessionVerdictV3Row, V3Candidate, V3Selection};

use super::config::SessionJudgeV3Config;
use super::rubric::RUBRIC_VERSION;
use super::run::{SessionJudgeFailure, SessionJudgementV3, SessionSubject, judge_session_v3};

pub struct LivePoll<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub embedding: &'a EmbeddingService,
    pub bundle_id: i64,
    pub lookback_hours: i64,
    pub max_attempts_per_day: i64,
    pub batch: i64,
    pub turn_judge_live_bundle_id: i64,
    pub turn_confidence_threshold: f32,
}

pub fn verdict_row<'a>(
    j: &'a SessionJudgementV3,
    cfg: &'a SessionJudgeV3Config,
    session_db_id: i64,
    mode: &'a str,
    bundle_id: Option<i64>,
) -> SessionVerdictV3Row<'a> {
    let end = j.end.as_ref();
    SessionVerdictV3Row {
        session_db_id,
        outcome_id: outcome_code_to_id(&j.verdict.outcome),
        quality_score: j.verdict.quality_score.clamp(0.0, 1.0),
        rationale: &j.verdict.rationale,
        judge_model: &cfg.model,
        rubric_version: RUBRIC_VERSION,
        mode,
        bundle_id,
        rubric_id: cfg.rubric_id,
        end_turn_id: end.and(j.facts.as_ref().map(|f| f.end_turn_id)),
        end_label: end.map(|e| e.label.code()),
        end_source: end.map(|e| {
            if e.deterministic.is_some() {
                "deterministic"
            } else {
                "llm"
            }
        }),
        end_rule: end.and_then(|e| e.deterministic).map(super::EndRule::code),
        end_confidence: end.map(|e| e.confidence),
        end_evidence: end.map(|e| e.evidence.as_str()),
        failure: end.map(|e| e.failure.as_str()),
        failure_tags: end.map(|e| e.failure_tags.clone()).unwrap_or_default(),
        severity: end.map(|e| e.severity),
        slices: cfg.slices.clone(),
        as_of: j.as_of,
        digest_version: &j.digest_version,
        input_text: &j.input_text,
        llm_output: serde_json::to_value(&j.verdict).unwrap_or_default(),
        generation_id: j.usage.generation_id.clone(),
        cost_usd: j.usage.cost_usd,
        prompt_tokens: j.usage.prompt_tokens,
        completion_tokens: j.usage.completion_tokens,
        latency_ms: j.usage.latency_ms,
    }
}

pub(super) async fn load_config(
    persist: &PersistHandle,
    bundle_id: i64,
) -> Option<SessionJudgeV3Config> {
    let row = ok_or_warn(
        persist.get_bundle(bundle_id).await,
        "session_judge_v3.read_bundle",
    )??;
    if row.status != "live" && row.status != "live_ungated" {
        crate::judge_worker::live_judge::mark_bundle_live(persist, bundle_id, ANNOUNCE_SOURCE)
            .await;
    }
    let mut cfg = ok_or_warn(
        SessionJudgeV3Config::from_bundle_components(&row.components),
        "session_judge_v3.bundle_components",
    )?;
    cfg.model =
        crate::llm::provider::resolve(Some(crate::llm::provider::ModelTier::Fast), &cfg.model);
    Some(cfg)
}

const ANNOUNCE_SOURCE: &str =
    r#"{"actor":"session_judge","reason":"session_judge.live_bundle_id"}"#;

impl LivePoll<'_> {
    async fn side_effects(&self, c: &V3Candidate, j: &SessionJudgementV3, verdict_id: i64) {
        let Ok(uuid) = c.session_uuid.parse::<Uuid>() else {
            return;
        };
        let overview = j.verdict.overview.trim();
        if !overview.is_empty() && j.verdict.outcome != "unknown" {
            write_overview(self.persist, uuid, &c.session_uuid, overview, Utc::now()).await;
        }
        write_knowledge(
            self.persist,
            uuid,
            &c.session_uuid,
            &j.verdict.legacy(),
            &[],
            Some(verdict_id),
        )
        .await;
        refresh_vector_post_verdict(self.persist, self.embedding, uuid, &c.session_uuid).await;
    }

    pub(super) async fn project(&self, turns: Vec<i64>) {
        if self.turn_judge_live_bundle_id <= 0 || turns.is_empty() {
            return;
        }
        let params = ProjectionParams {
            live_bundle_id: self.turn_judge_live_bundle_id,
            confidence_threshold: self.turn_confidence_threshold,
            mode: ProjectionMode::Apply,
            session_judge_live_bundle_id: self.bundle_id,
        };
        ok_or_warn(
            self.persist.project_turn_outcomes(&turns, &params).await,
            "session_judge_v3.projection",
        );
    }

    pub(super) async fn record(
        &self,
        session_db_id: i64,
        cfg: &SessionJudgeV3Config,
        failed: bool,
    ) {
        ok_or_warn(
            self.persist
                .record_judge_attempt(
                    session_db_id,
                    &cfg.model,
                    RUBRIC_VERSION,
                    Utc::now(),
                    failed,
                )
                .await,
            "session_judge_v3.record_attempt",
        );
    }

    async fn on_failure(
        &self,
        cfg: &SessionJudgeV3Config,
        c: &V3Candidate,
        breaker: &mut CircuitBreaker,
        e: &SessionJudgeFailure,
    ) -> bool {
        breaker.record(false, Instant::now());
        tracing::warn!(session_id = %c.session_uuid, error = %e, "session judge v3: judging failed");
        self.record(c.session_db_id, cfg, true).await;
        !breaker.should_skip(Instant::now())
    }

    async fn after_store(
        &self,
        c: &V3Candidate,
        judged: &SessionJudgementV3,
        verdict_id: i64,
        previous_end_turn: Option<i64>,
    ) {
        self.side_effects(c, judged, verdict_id).await;
        if judged.end.is_none() {
            return;
        }
        let mut turns = vec![c.end_turn_id];
        turns.extend(previous_end_turn.filter(|t| *t != c.end_turn_id));
        self.project(turns).await;
    }

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
        let previous_end_turn = ok_or_warn(
            self.persist.session_end_for_session(c.session_db_id).await,
            "session_judge_v3.previous_end",
        )
        .flatten()
        .map(|p| p.end_turn_id);
        let judged = match judge_session_v3(self.persist, &llm, cfg, &subject).await {
            Ok(j) => j,
            Err(SessionJudgeFailure::Refused) => return false,
            Err(e) => return self.on_failure(cfg, c, breaker, &e).await,
        };
        breaker.record(true, Instant::now());
        let row = verdict_row(&judged, cfg, c.session_db_id, "live", Some(self.bundle_id));
        let stored = ok_or_warn(
            self.persist.insert_session_verdict_v3(&row).await,
            "session_judge_v3.store_verdict",
        );
        self.record(c.session_db_id, cfg, stored.is_none()).await;
        if let Some(verdict_id) = stored {
            self.after_store(c, &judged, verdict_id, previous_end_turn)
                .await;
        }
        true
    }

    pub async fn run(&self, breaker: &mut CircuitBreaker) -> usize {
        if breaker.should_skip(Instant::now()) {
            return 0;
        }
        let Some(cfg) = load_config(self.persist, self.bundle_id).await else {
            return 0;
        };
        let selection = V3Selection {
            judge_model: &cfg.model,
            bundle_id: self.bundle_id,
            lookback_hours: i32::try_from(self.lookback_hours).unwrap_or(72),
            max_attempts_per_day: self.max_attempts_per_day,
            limit: self.batch,
        };
        let Some(candidates) = ok_or_warn(
            self.persist.fetch_sessions_needing_v3(&selection).await,
            "session_judge_v3.selection",
        ) else {
            return 0;
        };
        let mut judged = 0;
        for c in &candidates {
            if !self.one(&cfg, c, breaker).await {
                break;
            }
            judged += 1;
        }
        judged
    }
}
