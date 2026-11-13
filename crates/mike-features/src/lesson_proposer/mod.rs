use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub mod dedupe;
pub mod file_guidance;
pub mod guidance;
pub mod maintenance;
pub mod placement;
pub mod placement_pass;
pub mod prompt;
pub mod purpose;
pub mod quiet_budget;
pub mod relate;
pub mod replay;
pub mod select;
pub mod shape_pass;
pub mod shape_rewrite;
pub mod skill_author;
pub mod skill_eval;
pub mod skill_findings;
pub mod topics;
pub mod write;

#[cfg(test)]
pub mod maintenance_tests;
#[cfg(test)]
pub mod tests;

use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::analysis::embeddings::EmbeddingService;
use crate::dossier::view::types::AsOf;
use crate::llm::{LlmError, OpenRouterClient};
use crate::semantic::SemanticConfig;
use crate::semantic::neighbours::Retriever;
use crate::semantic::retrieve::{Audience, QuerySubject, RetrievedKind, Scope, SemanticQuery};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::proposals::CorrectionCandidate;

pub use prompt::{ProposedLesson, SimilarCorrection};
pub use quiet_budget::QuietBudget;
pub use write::WriteOutcome;

pub const DEFAULT_MODEL: &str = "google/gemini-3.5-flash-lite";
const SIMILAR_K: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct ProposerSettings {
    pub effective_model: String,
    pub min_confidence: f32,
    pub max_per_day: i64,
    pub maintenance_max_per_day: i64,
    pub quiet: QuietBudget,
    pub auto_promote_min_evidence: i64,
    pub max_chars: usize,
}

impl Default for ProposerSettings {
    fn default() -> Self {
        Self {
            effective_model: DEFAULT_MODEL.to_owned(),
            min_confidence: 0.7,
            max_per_day: 40,
            maintenance_max_per_day: 400,
            quiet: QuietBudget::default(),
            auto_promote_min_evidence: 2,
            max_chars: crate::lesson_shape::DEFAULT_MAX_CHARS,
        }
    }
}

fn non_empty_str(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

impl ProposerSettings {
    pub fn from_rows(rows: &[(String, Value)]) -> Self {
        let mut model_override = None;
        let mut session_judge_model = None;
        let mut settings = Self::default();
        for (key, value) in rows {
            match key.as_str() {
                "judge_lessons.model" => model_override = non_empty_str(value),
                "session_judge.model" => session_judge_model = non_empty_str(value),
                "judge_lessons.min_confidence" => {
                    if let Some(f) = value.as_f64() {
                        settings.min_confidence = f as f32;
                    }
                }
                "judge_lessons.max_per_day" => {
                    if let Some(n) = value.as_i64() {
                        settings.max_per_day = n.clamp(0, 1000);
                    }
                }
                "judge_lessons.maintenance_max_per_day" => {
                    if let Some(n) = value.as_i64() {
                        settings.maintenance_max_per_day = n.clamp(0, 5000);
                    }
                }
                "judge_lessons.auto_promote_min_evidence" => {
                    if let Some(n) = value.as_i64() {
                        settings.auto_promote_min_evidence = n.clamp(0, 100);
                    }
                }
                _ => settings.quiet.apply_row(key, value),
            }
        }
        settings.effective_model = model_override
            .or(session_judge_model)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        settings.max_chars = crate::lesson_shape::max_chars_from_rows(rows);
        settings
    }

    pub async fn load(persist: &PersistHandle) -> Self {
        persist
            .fetch_settings()
            .await
            .map(|rows| Self::from_rows(&rows))
            .unwrap_or_default()
    }
}

pub struct ProposerDeps<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub embedding: &'a EmbeddingService,
    pub semantic: &'a SemanticConfig,
    pub settings: &'a ProposerSettings,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct ProposerReport {
    pub candidates: usize,
    pub created: u64,
    pub reinforced: u64,
    pub evidence_only: u64,
    pub awaiting_embedding: u64,
    pub budget_exhausted: bool,
    pub promoted: u64,
    pub errors: u64,
    pub shape: shape_pass::ShapeReport,
    pub placement: placement_pass::PlacementReport,
    pub purpose: purpose::PurposeReport,
}

impl ProposerReport {
    fn record(&mut self, outcome: WriteOutcome) {
        match outcome {
            WriteOutcome::EvidenceOnly => self.evidence_only += 1,
            WriteOutcome::AwaitingEmbedding => self.awaiting_embedding += 1,
            WriteOutcome::Created { promoted, .. } => {
                self.created += 1;
                self.promoted += u64::from(promoted);
            }
            WriteOutcome::Reinforced { promoted, .. } => {
                self.reinforced += 1;
                self.promoted += u64::from(promoted);
            }
        }
    }
}

async fn select_candidates_or_log(deps: &ProposerDeps<'_>) -> Vec<CorrectionCandidate> {
    match select::select_candidates(deps.persist, deps.settings).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "lesson_proposer: candidate selection failed");
            Vec::new()
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CandidateError {
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error("{0}")]
    Other(String),
}

async fn examine_candidate(
    deps: &ProposerDeps<'_>,
    report: &mut ProposerReport,
    candidate: &CorrectionCandidate,
) -> bool {
    match process_one(deps, candidate).await {
        Ok(outcome) => {
            report.record(outcome);
            true
        }
        Err(error) => record_candidate_error(report, candidate.verdict_id, error),
    }
}

fn record_candidate_error(
    report: &mut ProposerReport,
    verdict_id: i64,
    error: CandidateError,
) -> bool {
    if matches!(&error, CandidateError::Llm(cause) if cause.is_refusal()) {
        report.budget_exhausted = true;
        log_candidate_refused(&error);
        return false;
    }
    report.errors += 1;
    log_candidate_failed(verdict_id, &error);
    true
}

fn log_candidate_refused(error: &CandidateError) {
    tracing::debug!(%error, "lesson_proposer: limiter refused, deferring remaining candidates");
}

fn log_candidate_failed(verdict_id: i64, error: &CandidateError) {
    tracing::warn!(verdict_id, %error, "lesson_proposer: examining candidate failed");
}

pub async fn run_tick(deps: &ProposerDeps<'_>) -> ProposerReport {
    let mut report = ProposerReport::default();
    let _guard = match deps.persist.lessons().lock_lesson_proposer().await {
        Ok(Some(guard)) => guard,
        Ok(None) => return report,
        Err(error) => {
            tracing::warn!(%error, "lesson_proposer: failed to lock worker");
            report.errors += 1;
            return report;
        }
    };
    replay::retry_pending(deps, &mut report).await;
    examine_candidates(deps, &mut report).await;
    report.shape = shape_pass::run_pass(deps).await;
    report.placement = placement_pass::run_pass(deps).await;
    report.purpose = purpose::run_pass(deps).await;
    report
}

async fn examine_candidates(deps: &ProposerDeps<'_>, report: &mut ProposerReport) {
    let candidates = select_candidates_or_log(deps).await;
    report.candidates = candidates.len();
    let daily_limit = deps
        .settings
        .quiet
        .effective_limit(deps.persist, deps.settings.max_per_day)
        .await;
    let mut calls_this_tick = 0;
    for candidate in &candidates {
        if !reserve_candidate(deps, candidate, report, daily_limit, &mut calls_this_tick).await {
            break;
        }
        if !examine_candidate(deps, report, candidate).await {
            break;
        }
    }
}

async fn reserve_candidate(
    deps: &ProposerDeps<'_>,
    candidate: &CorrectionCandidate,
    report: &mut ProposerReport,
    daily_limit: i64,
    calls_this_tick: &mut usize,
) -> bool {
    if candidate.confidence < deps.settings.min_confidence {
        return true;
    }
    if deps.llm.budget().calls_now == 0 {
        report.budget_exhausted = true;
        return false;
    }
    if *calls_this_tick >= 40 {
        return false;
    }
    match deps
        .persist
        .lessons()
        .reserve_lesson_proposer_call(daily_limit)
        .await
    {
        Ok(true) => {
            *calls_this_tick += 1;
            true
        }
        Ok(false) => {
            report.budget_exhausted = true;
            false
        }
        Err(error) => {
            tracing::warn!(%error, "lesson_proposer: budget reservation failed");
            report.errors += 1;
            false
        }
    }
}

async fn embedding_for_rule(
    embedding: &EmbeddingService,
    rule: Option<&str>,
) -> Option<dedupe::Embedded> {
    let rule = rule?;
    match dedupe::embed_rule(embedding, rule).await {
        Ok(embedded) => Some(embedded),
        Err(e) => {
            tracing::debug!(error = %e, "lesson_proposer: rule embedding unavailable");
            None
        }
    }
}

async fn process_one(
    deps: &ProposerDeps<'_>,
    candidate: &CorrectionCandidate,
) -> Result<WriteOutcome, CandidateError> {
    if candidate.confidence < deps.settings.min_confidence {
        return write::record_low_confidence_evidence(deps.persist, candidate)
            .await
            .map(|()| WriteOutcome::EvidenceOnly)
            .map_err(|e| CandidateError::Other(e.to_string()));
    }
    let similar = similar_corrections(deps, candidate).await;
    let similar_json = similar_to_json(&similar);
    let shown = guidance::closest(deps, candidate).await;
    let (proposed, usage) = prompt::call_proposer(deps.llm, candidate, &similar, &shown).await?;

    let saved = crate::store::persistence::lessons::proposals::queue::SavedProposal {
        candidate: candidate.clone(),
        proposed,
        similar: similar_json,
        model: deps.llm.model().to_owned(),
        cost_usd: usage.cost_usd,
        generation_id: usage.generation_id,
    };
    deps.persist
        .lessons()
        .save_lesson_proposal(&saved)
        .await
        .map_err(|e| CandidateError::Other(e.to_string()))?;
    guidance::record(deps, candidate, &shown, &saved.proposed.reason).await;
    replay::apply_saved(deps, &saved)
        .await
        .map_err(CandidateError::Other)
}

async fn similar_corrections(
    deps: &ProposerDeps<'_>,
    candidate: &CorrectionCandidate,
) -> Vec<SimilarCorrection> {
    let retriever = Retriever {
        persist: deps.persist,
        embedding: Some(deps.embedding),
        config: deps.semantic,
        live_consumer: true,
    };
    let query = SemanticQuery {
        subject: QuerySubject::ClosedTurn(candidate.operator_turn_id),
        audience: Audience::Internal,
        kinds: vec![RetrievedKind::Correction],
        scope: Scope::Project,
        as_of: AsOf::knowledge(Utc::now()),
        k: SIMILAR_K,
        offset: 0,
    };
    match retriever.neighbours(&query).await {
        Ok(set) => set
            .neighbours
            .into_iter()
            .map(|n| SimilarCorrection {
                item_id: n.item_id,
                similarity: n.similarity,
                origin_session_id: n.row.session_id,
                origin_turn_id: n.row.operator_turn_id,
                text: n.row.text,
            })
            .collect(),
        Err(e) => {
            tracing::debug!(error = %e, "lesson_proposer: similar-corrections lookup failed");
            Vec::new()
        }
    }
}

fn similar_to_json(similar: &[SimilarCorrection]) -> Value {
    json!(
        similar
            .iter()
            .map(|s| json!({
                "item_id": s.item_id,
                "similarity": s.similarity,
                "text": s.text,
                "origin_session_id": s.origin_session_id,
                "origin_turn_id": s.origin_turn_id,
            }))
            .collect::<Vec<_>>()
    )
}
