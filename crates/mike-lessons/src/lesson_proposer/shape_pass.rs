use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use serde::Serialize;
use serde_json::json;

use crate::lesson_shape::{Assessment, assess};
use crate::llm::LlmError;
use crate::store::persistence::PersistError;
use crate::store::persistence::lessons::maintenance_budget::{MaintenancePass, RefusalReason};
use crate::store::persistence::lessons::shape::ShapeWork;
use crate::store::persistence::lessons::shape_apply::{RewrittenRule, RuleEmbedding, ShapeRewrite};
use crate::store::persistence::session_notes::NewSessionNote;

use super::maintenance::{self, Reservation};
use super::relate::{self, Relation};
use super::{ProposerDeps, dedupe, shape_rewrite};

#[cfg(test)]
#[path = "shape_pass_tests.rs"]
mod tests;

const SCAN_BATCH: i64 = 500;
pub const REWRITES_PER_TICK: i64 = 3;
const VERIFIER_TOPIC_PREFIX: &str = "verifier:";
const RAW_STAGE: &str = "raw";
const CONSOLIDATED_SOURCES: [&str; 4] = ["operator", "harvester", "agent", "copilot"];
const STATUS_SHAPE: &str = "status";
const NO_RULE_FOR_LONG_TEXT: &str = "the model returned no rule for an over-length lesson";

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct ShapeReport {
    pub checked: u64,
    pub flagged: u64,
    pub rewritten: u64,
    pub replacements: u64,
    pub retired: u64,
    pub failed: u64,
    pub kept: u64,
    pub budget_exhausted: bool,
}

enum Handled {
    Continue,
    Stop,
}

fn log_persist_failed(step: &str, error: &PersistError) {
    tracing::warn!(%error, step, "lesson_proposer: shape pass could not reach the store");
}

fn log_rewrite_failed(lesson_id: i64, message: &str) {
    tracing::warn!(
        lesson_id,
        error = message,
        "lesson_proposer: shape rewrite failed"
    );
}

fn log_handover_failed(lesson_id: i64, error: &PersistError) {
    tracing::warn!(%error, lesson_id, "lesson_proposer: handover note could not be written");
}

pub async fn run_pass(deps: &ProposerDeps<'_>) -> ShapeReport {
    let mut report = ShapeReport::default();
    scan(deps, &mut report).await;
    let work = claim(deps).await;
    if work.is_empty() {
        return report;
    }
    for item in &work {
        if let Handled::Stop = handle_one(deps, &mut report, item).await {
            break;
        }
    }
    report
}

async fn claim(deps: &ProposerDeps<'_>) -> Vec<ShapeWork> {
    deps.persist
        .lessons()
        .claim_shape_work(REWRITES_PER_TICK)
        .await
        .unwrap_or_else(|error| {
            log_persist_failed("claim_shape_work", &error);
            Vec::new()
        })
}

async fn unchecked(deps: &ProposerDeps<'_>) -> Vec<(i64, Assessment)> {
    let rows = deps
        .persist
        .lessons()
        .unchecked_lessons_for_shape(SCAN_BATCH)
        .await
        .unwrap_or_else(|error| {
            log_persist_failed("unchecked_lessons_for_shape", &error);
            Vec::new()
        });
    let mut findings: Vec<(i64, Assessment)> = rows
        .iter()
        .map(|lesson| {
            let found = assess(
                &lesson.text,
                lesson.topic.as_deref(),
                deps.settings.max_chars,
            );
            (lesson.id, found)
        })
        .collect();
    super::shape_gate::clear_false_status(deps, &rows, &mut findings).await;
    findings
}

async fn scan(deps: &ProposerDeps<'_>, report: &mut ShapeReport) {
    let findings = unchecked(deps).await;
    match deps
        .persist
        .lessons()
        .record_shape_findings(&findings)
        .await
    {
        Ok(_) => {
            report.checked += findings.len() as u64;
            report.flagged += findings.iter().filter(|(_, a)| !a.is_rule()).count() as u64;
        }
        Err(error) => log_persist_failed("record_shape_findings", &error),
    }
}

fn is_verifier_note(work: &ShapeWork) -> bool {
    work.topic
        .as_deref()
        .is_some_and(|topic| topic.starts_with(VERIFIER_TOPIC_PREFIX))
}

async fn handle_one(
    deps: &ProposerDeps<'_>,
    report: &mut ShapeReport,
    work: &ShapeWork,
) -> Handled {
    if is_verifier_note(work) {
        retire(
            deps,
            report,
            work,
            "the topic marks a verifier carryover note",
        )
        .await;
        return Handled::Continue;
    }
    match maintenance::reserve(deps, MaintenancePass::Shape).await {
        Reservation::Reserved => {}
        Reservation::Refused => {
            report.budget_exhausted = true;
            return Handled::Stop;
        }
        Reservation::Failed => {
            report.failed += 1;
            return Handled::Stop;
        }
    }
    match shape_rewrite::call(deps.llm, work, deps.settings.max_chars).await {
        Ok((answer, _usage)) => apply_answer(deps, report, work, &answer).await,
        Err(error) => on_call_error(deps, report, work, &error).await,
    }
}

async fn on_call_error(
    deps: &ProposerDeps<'_>,
    report: &mut ShapeReport,
    work: &ShapeWork,
    error: &LlmError,
) -> Handled {
    if error.is_refusal() {
        maintenance::note_refusal(deps, MaintenancePass::Shape, RefusalReason::ProviderLimit).await;
        report.budget_exhausted = true;
        return Handled::Stop;
    }
    fail(deps, report, work, &error.to_string()).await;
    Handled::Continue
}

async fn apply_answer(
    deps: &ProposerDeps<'_>,
    report: &mut ShapeReport,
    work: &ShapeWork,
    answer: &shape_rewrite::RewriteAnswer,
) -> Handled {
    match shape_rewrite::accepted_rules(&answer.rules, deps.settings.max_chars) {
        Err(message) => fail(deps, report, work, &message).await,
        Ok(rules) if rules.is_empty() => no_rule(deps, report, work, &answer.reason).await,
        Ok(rules) => rewrite(deps, report, work, &rules, &answer.reason).await,
    }
    Handled::Continue
}

async fn no_rule(
    deps: &ProposerDeps<'_>,
    report: &mut ShapeReport,
    work: &ShapeWork,
    reason: &str,
) {
    if work.shape == STATUS_SHAPE {
        retire(deps, report, work, reason).await;
    } else {
        fail(deps, report, work, NO_RULE_FOR_LONG_TEXT).await;
    }
}

fn awaits_consolidation(work: &ShapeWork) -> bool {
    work.stage == RAW_STAGE && CONSOLIDATED_SOURCES.contains(&work.source.as_str())
}

async fn embed_rule(
    deps: &ProposerDeps<'_>,
    work: &ShapeWork,
    text: &str,
) -> Option<RuleEmbedding> {
    if awaits_consolidation(work) {
        return None;
    }
    let embedded = dedupe::embed_rule(deps.embedding, text).await.ok()?;
    Some(RuleEmbedding {
        vector: embedded.vector,
        model: embedded.model,
    })
}

async fn embed_rules(
    deps: &ProposerDeps<'_>,
    work: &ShapeWork,
    rules: &[String],
) -> Vec<RewrittenRule> {
    let mut out = Vec::with_capacity(rules.len());
    for text in rules {
        out.push(RewrittenRule {
            text: text.clone(),
            embedding: embed_rule(deps, work, text).await,
        });
    }
    out
}

pub struct TwinDecision {
    pub lesson_id: i64,
    pub relation: Relation,
    pub reason: String,
}

async fn twin_decisions(
    deps: &ProposerDeps<'_>,
    work: &ShapeWork,
    rule: &RewrittenRule,
) -> Vec<TwinDecision> {
    let (Some(project_id), Some(embedding)) = (work.project_id, rule.embedding.as_ref()) else {
        return Vec::new();
    };
    let neighbours: Vec<_> = deps
        .persist
        .lessons()
        .find_lesson_neighbours(
            project_id,
            &embedding.vector,
            relate::NEIGHBOUR_MAX_DISTANCE,
            relate::NEIGHBOUR_LIMIT,
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.id != work.lesson_id)
        .collect();
    if neighbours.is_empty() {
        return Vec::new();
    }
    let daily_limit = deps
        .settings
        .quiet
        .effective_limit(deps.persist, deps.settings.max_per_day)
        .await;
    if !deps
        .persist
        .lessons()
        .reserve_lesson_proposer_call(daily_limit)
        .await
        .unwrap_or(false)
    {
        return Vec::new();
    }
    match relate::call(
        deps.llm,
        &rule.text,
        &neighbours,
        &[],
        deps.settings.max_chars,
    )
    .await
    {
        Ok((answer, _usage)) => answer
            .decisions
            .into_iter()
            .filter(|d| neighbours.iter().any(|n| n.id == d.lesson_id))
            .map(|d| TwinDecision {
                lesson_id: d.lesson_id,
                relation: d.decision,
                reason: d.reason,
            })
            .collect(),
        Err(error) => {
            tracing::debug!(lesson_id = work.lesson_id, %error, "shape pass: the twin comparison did not answer; the rule is stored");
            Vec::new()
        }
    }
}

pub fn kept_after_comparison(
    rules: Vec<RewrittenRule>,
    decisions: &[Vec<TwinDecision>],
) -> (Vec<RewrittenRule>, Vec<String>) {
    let mut kept = Vec::new();
    let mut notes = Vec::new();
    for (index, rule) in rules.into_iter().enumerate() {
        let repeats = decisions
            .get(index)
            .and_then(|d| d.iter().find(|d| d.relation == Relation::Skip));
        match repeats {
            Some(twin) => notes.push(format!(
                "rule {} not stored, it repeats lesson {}: {}",
                index + 1,
                twin.lesson_id,
                twin.reason.trim()
            )),
            None => kept.push(rule),
        }
    }
    (kept, notes)
}

async fn compared(
    deps: &ProposerDeps<'_>,
    work: &ShapeWork,
    rules: Vec<RewrittenRule>,
) -> (Vec<RewrittenRule>, Vec<String>) {
    let mut decisions = Vec::with_capacity(rules.len());
    for rule in &rules {
        decisions.push(twin_decisions(deps, work, rule).await);
    }
    kept_after_comparison(rules, &decisions)
}

async fn rewrite(
    deps: &ProposerDeps<'_>,
    report: &mut ShapeReport,
    work: &ShapeWork,
    rules: &[String],
    reason: &str,
) {
    let (embedded, notes) = compared(deps, work, embed_rules(deps, work, rules).await).await;
    let rationale = if notes.is_empty() {
        reason.to_string()
    } else {
        format!("{reason} {}", notes.join(" "))
    };
    let rewrite = ShapeRewrite {
        lesson_id: work.lesson_id,
        rules: &embedded,
        model: deps.llm.model(),
        original_chars: work.text.chars().count(),
        rationale: &rationale,
    };
    match deps
        .persist
        .lessons()
        .rewrite_lesson_into_rules(&rewrite)
        .await
    {
        Ok(Some(ids)) => {
            report.rewritten += 1;
            report.replacements += ids.len() as u64;
        }
        Ok(None) => {}
        Err(error) => fail(deps, report, work, &error.to_string()).await,
    }
}

async fn fail(deps: &ProposerDeps<'_>, report: &mut ShapeReport, work: &ShapeWork, message: &str) {
    log_rewrite_failed(work.lesson_id, message);
    report.failed += 1;
    match deps
        .persist
        .lessons()
        .record_shape_failure(work.lesson_id, message)
        .await
    {
        Ok(kept) => report.kept += u64::from(kept),
        Err(error) => log_persist_failed("record_shape_failure", &error),
    }
}

async fn retire(deps: &ProposerDeps<'_>, report: &mut ShapeReport, work: &ShapeWork, reason: &str) {
    match deps
        .persist
        .lessons()
        .retire_lesson_as_not_a_lesson(work.lesson_id)
        .await
    {
        Ok(true) => {
            report.retired += 1;
            write_handover_note(deps, work, reason).await;
        }
        Ok(false) => {}
        Err(error) => fail(deps, report, work, &error.to_string()).await,
    }
}

fn handover_body(work: &ShapeWork, reason: &str) -> String {
    format!(
        "{}\n\nThis text was stored as a {} lesson. It reports on a session instead of stating a rule, so it was archived as not_a_lesson ({reason}). The original stays readable on lesson {}.",
        work.text, work.kind, work.lesson_id
    )
}

async fn write_handover_note(deps: &ProposerDeps<'_>, work: &ShapeWork, reason: &str) {
    let Some(session_uuid) = work.session_uuid else {
        return;
    };
    let title = format!(
        "Lesson {} moved out of lessons: a status note, not a rule",
        work.lesson_id
    );
    let body = handover_body(work, reason);
    let tags = vec![
        "not_a_lesson".to_owned(),
        format!("lesson:{}", work.lesson_id),
    ];
    let data = json!({"lesson_id": work.lesson_id, "topic": work.topic, "stage": work.stage});
    let note = NewSessionNote {
        kind: Some("handover"),
        title: &title,
        body: Some(&body),
        status: None,
        tags: Some(&tags),
        data: Some(&data),
    };
    if let Err(error) = deps
        .persist
        .fleet()
        .create_session_note(session_uuid, note)
        .await
    {
        log_handover_failed(work.lesson_id, &error);
    }
}
