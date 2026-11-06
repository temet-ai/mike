use serde::Serialize;
use serde_json::{Value, json};

use super::skill_author::{AuthoredReference, TriggerQuery};
use super::skill_findings::Findings;
use crate::llm::{LlmError, OpenRouterClient};
use crate::store::persistence::lessons::skill_draft_eval::{
    EvalDraft, EvalFailure, ShippedPatch, SiblingSkill,
};
use crate::store::persistence::{PersistError, PersistHandle};

#[path = "skill_eval_breaker.rs"]
mod breaker;
#[path = "skill_eval_catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "skill_eval_catalog_tests.rs"]
mod catalog_tests;
#[path = "skill_eval_overlap.rs"]
mod overlap;
#[cfg(test)]
#[path = "skill_eval_overlap_tests.rs"]
mod overlap_tests;
#[path = "skill_eval_patch.rs"]
mod patching;
#[path = "skill_eval_progress.rs"]
mod progress;
#[path = "skill_eval_review.rs"]
mod review;
#[cfg(test)]
#[path = "skill_eval_review_tests.rs"]
mod review_tests;
#[cfg(test)]
#[path = "skill_eval_tests.rs"]
mod tests;
#[path = "skill_eval_trigger.rs"]
mod trigger;
#[cfg(test)]
#[path = "skill_eval_trigger_tests.rs"]
mod trigger_tests;
#[path = "skill_eval_verdict.rs"]
mod verdict;
#[path = "skill_eval_workspace.rs"]
mod workspace;

pub use verdict::MIN_TRIGGER_SCORE;

use progress::{Progress, Unit};
use review::{Decision, Review};

pub const MAX_ATTEMPTS: i32 = 3;
pub const SIBLINGS: i64 = 5;
const PARALLEL_CALLS: usize = 4;
pub const EVALUATOR_VERSION: i32 = 5;

#[derive(Debug, thiserror::Error)]
enum EvalError {
    #[error("{0}")]
    Deferred(String),
    #[error("draft cannot be evaluated: {0}")]
    Invalid(String),
    #[error("model: {0}")]
    Model(String),
}

impl From<LlmError> for EvalError {
    fn from(error: LlmError) -> Self {
        if error.is_refusal() {
            Self::Deferred(error.to_string())
        } else {
            Self::Model(error.to_string())
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct EvalPassReport {
    pub draft_id: Option<i64>,
    pub folder: Option<String>,
    pub trigger_batches_done: usize,
    pub reviewed: bool,
    pub calls: u32,
    pub accepted: Option<bool>,
    pub trigger_score: Option<f64>,
    pub patched_trigger_score: Option<f64>,
    pub deferred: bool,
    pub requeued: i64,
    pub error: Option<String>,
}

struct Material {
    name: String,
    description: String,
    skill_md: String,
    references: Vec<AuthoredReference>,
    queries: Vec<TriggerQuery>,
    lesson_ids: Vec<i64>,
    agent_id: Option<i64>,
}

#[derive(Serialize)]
struct Evaluation {
    evaluator_version: i32,
    models: Vec<String>,
    calls: u32,
    siblings: Vec<SiblingSkill>,
    real_skills: catalog::RealSkills,
    workspace: workspace::Workspace,
    trigger: trigger::TriggerEval,
    review: Review,
    recheck: Option<trigger::TriggerEval>,
    verdict: verdict::Verdict,
    findings: Findings,
}

enum Step {
    Saved,
    Decided(Box<Evaluation>),
}

impl Material {
    fn parse(draft: &EvalDraft) -> Result<Self, EvalError> {
        let references: Vec<AuthoredReference> =
            serde_json::from_value(draft.reference_files.clone())
                .map_err(|e| EvalError::Invalid(format!("reference_files: {e}")))?;
        let queries: Vec<TriggerQuery> = serde_json::from_value(draft.trigger_queries.clone())
            .map_err(|e| EvalError::Invalid(format!("trigger_queries: {e}")))?;
        if queries.is_empty() {
            return Err(EvalError::Invalid("trigger_queries is empty".to_owned()));
        }
        Ok(Self {
            name: draft.folder.clone(),
            description: draft.description.clone(),
            skill_md: draft.skill_md.clone(),
            references,
            queries,
            lesson_ids: draft.lesson_ids.clone(),
            agent_id: draft.agent_id,
        })
    }
}

fn evaluation(progress: Progress, review: Review) -> Evaluation {
    let patched = review.decision == Decision::Patch;
    let first = trigger::score(progress.trigger);
    let recheck = patched.then(|| trigger::score(progress.recheck));
    let verdict = verdict::decide(&first, recheck.as_ref(), &review);
    let findings = verdict::findings(&first, recheck.as_ref());
    Evaluation {
        evaluator_version: EVALUATOR_VERSION,
        models: progress.models,
        calls: progress.calls,
        siblings: progress.siblings.unwrap_or_default(),
        real_skills: progress.real_skills.unwrap_or_default(),
        workspace: progress.workspace.unwrap_or_default(),
        trigger: first,
        review,
        recheck,
        verdict,
        findings,
    }
}

async fn competitors_of(
    persist: &PersistHandle,
    draft: &EvalDraft,
    progress: &mut Progress,
) -> Result<(), EvalError> {
    if progress.siblings.is_none() {
        let found = persist
            .skill_draft_siblings(draft.project_id, &draft.folder, SIBLINGS)
            .await
            .map_err(|e| EvalError::Deferred(format!("siblings: {e}")))?;
        progress.siblings = Some(found);
    }
    if progress.real_skills.is_none() {
        let found = catalog::load(persist, draft.project_id, &draft.folder, draft.id)
            .await
            .map_err(|e| EvalError::Deferred(format!("project skills: {e}")))?;
        progress.real_skills = Some(found);
    }
    Ok(())
}

fn entries_for(progress: &Progress, name: &str, description: &str) -> Vec<(String, String)> {
    let siblings = progress.siblings.as_deref().unwrap_or_default();
    let real = progress
        .real_skills
        .as_ref()
        .map(|r| r.skills.as_slice())
        .unwrap_or_default();
    trigger::entries(name, description, siblings, real)
}

async fn trigger_phase(
    llm: &OpenRouterClient,
    material: &Material,
    progress: &mut Progress,
    plan: &[Unit],
) -> Result<(), EvalError> {
    let again = matches!(plan.first(), Some(Unit::Recheck(_)));
    let description = progress
        .review
        .as_ref()
        .and_then(|r| r.patched_description.clone())
        .filter(|_| again)
        .unwrap_or_else(|| material.description.clone());
    let entries = entries_for(progress, &material.name, &description);
    let project = workspace::text_of(progress);
    let offer = trigger::Offer {
        name: &material.name,
        entries: &entries,
        project: &project,
    };
    let batches = progress::batch_ids(plan);
    let results = trigger::run_batches(llm, &offer, &material.queries, &batches).await;
    let (answered, error) = progress::settled(results);
    let done = answered.len();
    let kept = answered.into_iter().flatten();
    if again {
        progress.recheck.extend(kept);
        progress.recheck_batches += done;
    } else {
        progress.trigger.extend(kept);
        progress.trigger_batches += done;
    }
    error.map_or(Ok(()), Err)
}

async fn review_phase(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    draft: &EvalDraft,
    material: &Material,
    progress: &mut Progress,
) -> Result<(), EvalError> {
    let lessons = persist
        .skill_draft_lessons_by_id(&material.lesson_ids)
        .await
        .map_err(|e| EvalError::Deferred(format!("lessons: {e}")))?;
    let agent = match material.agent_id {
        Some(id) => persist
            .skill_draft_agent_scope(id)
            .await
            .map_err(|e| EvalError::Deferred(format!("agent: {e}")))?,
        None => None,
    };
    let own = overlap::own_folders(&material.name, agent.as_ref());
    let replaced = persist
        .skill_draft_replaced(draft.project_id, &own, draft.id)
        .await
        .map_err(|e| EvalError::Deferred(format!("replaced skill: {e}")))?;
    let skills = overlap::existing_skills(progress, &own);
    let project = workspace::text_of(progress);
    let context = review::Context {
        skills: &skills,
        replaced: replaced.as_ref(),
        agent: agent.as_ref(),
        project: &project,
    };
    let first = trigger::score(progress.trigger.clone());
    progress.review = Some(review::review(llm, material, &lessons, &first, &context).await?);
    Ok(())
}

async fn work(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    draft: &EvalDraft,
    material: &Material,
    progress: &mut Progress,
) -> Result<(), EvalError> {
    let reservation = llm.reserve(progress.worst_case_calls(material));
    let mut budget = reservation.granted();
    let client = llm.with_reservation(&reservation);
    progress.note_model(llm.model());
    competitors_of(persist, draft, progress).await?;
    workspace::ensure(persist, draft, material, progress).await?;
    let mut ran = false;
    loop {
        let plan = progress::plan(&progress.pending(material), budget);
        let Some(first) = plan.first().copied() else {
            break;
        };
        ran = true;
        let spent = progress::cost(&plan);
        budget = budget.saturating_sub(spent);
        progress.calls += spent;
        match first {
            Unit::Review => review_phase(persist, &client, draft, material, progress).await?,
            Unit::Batch(_) | Unit::Recheck(_) => {
                trigger_phase(&client, material, progress, &plan).await?;
            }
        }
    }
    if ran || progress.pending(material).is_empty() {
        return Ok(());
    }
    Err(EvalError::Deferred(format!(
        "{} calls available",
        reservation.granted()
    )))
}

async fn advance(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    draft: &EvalDraft,
    report: &mut EvalPassReport,
) -> Result<Step, EvalError> {
    let material = Material::parse(draft)?;
    let mut progress = Progress::from_row(draft.eval_progress.as_ref());
    let outcome = if progress.pending(&material).is_empty() {
        Ok(())
    } else {
        work(persist, llm, draft, &material, &mut progress).await
    };
    report.trigger_batches_done = progress.trigger_batches;
    report.reviewed = progress.review.is_some();
    report.calls = progress.calls;
    save_progress(persist, draft, &progress).await;
    outcome?;
    match progress.review.clone() {
        Some(review) if progress.pending(&material).is_empty() => {
            Ok(Step::Decided(Box::new(evaluation(progress, review))))
        }
        _ => Ok(Step::Saved),
    }
}

pub async fn run_pass(persist: &PersistHandle, llm: &OpenRouterClient) -> EvalPassReport {
    let mut report = EvalPassReport {
        requeued: requeue_stale(persist).await,
        ..EvalPassReport::default()
    };
    if llm.budget().calls_now == 0 || !breaker::LOOP.allows(breaker::now_secs()) {
        report.deferred = true;
        return report;
    }
    let draft = match persist.claim_skill_draft_for_eval().await {
        Ok(Some(draft)) => draft,
        Ok(None) => return report,
        Err(e) => {
            report.error = Some(format!("claim: {e}"));
            return report;
        }
    };
    report.draft_id = Some(draft.id);
    report.folder = Some(draft.folder.clone());
    let outcome = advance(persist, llm, &draft, &mut report).await;
    let report = settle(persist, &draft, outcome, report).await;
    tracing::debug!(?report, "skill eval: pass finished");
    report
}

async fn requeue_stale(persist: &PersistHandle) -> i64 {
    let requeued = persist
        .requeue_skill_drafts_before_evaluator(EVALUATOR_VERSION)
        .await
        .unwrap_or_else(|e| {
            log_requeue_failed(&e);
            0
        });
    log_requeued(requeued);
    requeued
}

fn log_requeued(requeued: i64) {
    if requeued > 0 {
        tracing::info!(
            requeued,
            evaluator_version = EVALUATOR_VERSION,
            "skill eval: drafts rejected by an older evaluator put back in the queue"
        );
    }
}

fn log_requeue_failed(error: &PersistError) {
    tracing::warn!(%error, "skill eval: older rejections not requeued");
}

async fn settle(
    persist: &PersistHandle,
    draft: &EvalDraft,
    outcome: Result<Step, EvalError>,
    mut report: EvalPassReport,
) -> EvalPassReport {
    match outcome {
        Ok(Step::Saved) => breaker::LOOP.succeeded(),
        Ok(Step::Decided(evaluation)) => {
            breaker::LOOP.succeeded();
            store(persist, draft, &evaluation, &mut report).await;
        }
        Err(EvalError::Deferred(reason)) => {
            report.deferred = true;
            tracing::debug!(%reason, folder = %draft.folder, "skill eval: deferred");
        }
        Err(EvalError::Invalid(reason)) => {
            let eval = json!({"evaluator_version": EVALUATOR_VERSION, "error": reason});
            finish(persist, draft, false, &eval, None).await;
            report.accepted = Some(false);
            report.error = Some(reason);
        }
        Err(EvalError::Model(reason)) => {
            breaker::LOOP.failed(breaker::now_secs());
            record_failure(persist, draft, &reason).await;
            report.error = Some(reason);
        }
    }
    report
}

async fn store(
    persist: &PersistHandle,
    draft: &EvalDraft,
    evaluation: &Evaluation,
    report: &mut EvalPassReport,
) {
    let accepted = evaluation.verdict.accepted;
    let eval = serde_json::to_value(evaluation).unwrap_or(Value::Null);
    let review = &evaluation.review;
    let patch = match (&review.patched_description, &review.patched_skill_md) {
        (Some(description), Some(skill_md)) if accepted => Some(ShippedPatch {
            description,
            skill_md,
        }),
        _ => None,
    };
    finish(persist, draft, accepted, &eval, patch).await;
    report.accepted = Some(accepted);
    report.trigger_score = Some(evaluation.trigger.score);
    report.patched_trigger_score = evaluation.recheck.as_ref().map(|t| t.score);
    tracing::info!(
        draft_id = draft.id,
        folder = %draft.folder,
        accepted,
        calls = evaluation.calls,
        trigger_score = evaluation.trigger.score,
        decision = ?review.decision,
        reasons = ?evaluation.verdict.reasons,
        "skill eval: draft decided"
    );
}

async fn save_progress(persist: &PersistHandle, draft: &EvalDraft, progress: &Progress) {
    let value = serde_json::to_value(progress).unwrap_or(Value::Null);
    if let Err(e) = persist.save_skill_draft_progress(draft.id, &value).await {
        tracing::warn!(error = %e, draft_id = draft.id, "skill eval: progress not saved");
    }
}

async fn finish(
    persist: &PersistHandle,
    draft: &EvalDraft,
    accepted: bool,
    eval: &Value,
    patch: Option<ShippedPatch<'_>>,
) {
    if let Err(e) = persist
        .finish_skill_draft_eval(draft.id, accepted, eval, patch)
        .await
    {
        tracing::warn!(error = %e, draft_id = draft.id, "skill eval: result not stored");
    }
}

async fn record_failure(persist: &PersistHandle, draft: &EvalDraft, reason: &str) {
    let recorded = persist
        .fail_skill_draft_eval(draft.id, reason, MAX_ATTEMPTS, EVALUATOR_VERSION)
        .await;
    match recorded {
        Ok(failure) => log_attempt_failed(draft, reason, failure),
        Err(e) => log_failure_lost(draft, &e),
    }
}

fn log_attempt_failed(draft: &EvalDraft, reason: &str, failure: Option<EvalFailure>) {
    tracing::warn!(
        draft_id = draft.id,
        folder = %draft.folder,
        reason,
        ?failure,
        "skill eval: attempt failed"
    );
}

fn log_failure_lost(draft: &EvalDraft, error: &PersistError) {
    tracing::warn!(%error, draft_id = draft.id, "skill eval: failure not recorded");
}
