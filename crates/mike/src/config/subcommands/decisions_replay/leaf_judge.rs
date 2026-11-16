use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::launch::fleet::leaves::check_output::excerpt;
use crate::launch::fleet::leaves::check_tests::{candidate_files, test_names};
use crate::launch::fleet::leaves::judge::{CriterionText, ReportFacts};
use crate::launch::fleet::leaves::judge_jev::{
    Composed, PASS_AT, POINT, QUESTION_SET, decide, questions,
};
use crate::launch::fleet::watch::git;
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;

const PASS: &str = "pass";
const CHECK: &str = "check";
const REPOS_ENV: &str = "MIKE_REPLAY_REPOS";
const OUTPUT_AS_OF: &str = "SELECT r.content FROM tools.uses u \
       JOIN tools.results r ON r.tool_use_id = u.id \
      WHERE u.tool_name = 'Bash' AND u.input->>'command' ILIKE '%' || $1 || '%' \
        AND u.created_at <= $2 AND u.created_at >= $2 - interval '24 hours' \
      ORDER BY u.id DESC LIMIT 1";

pub(super) struct Ready {
    pub record: Labelled,
    pub criteria: Vec<CriterionText>,
    pub report: ReportFacts,
    pub expected: String,
}

pub(super) struct LeafJudge;

fn verdict_code(composed: &Composed) -> &'static str {
    match composed {
        Composed::Pass { .. } => PASS,
        Composed::Fail { .. } => "fail",
        Composed::Check => CHECK,
    }
}

fn old_verdict(record: &Labelled, criteria: &[CriterionText], report: &ReportFacts) -> String {
    let all_met = criteria.iter().all(|c| {
        record
            .old_noul(&format!("criterion_{}", c.id))
            .is_some_and(|p| p >= PASS_AT)
    });
    let clean = report.exercised_exit == Some(0) && report.unestablished.is_empty();
    if all_met && clean { PASS } else { CHECK }.to_owned()
}

fn expected(record: &Labelled, criteria: &[CriterionText], report: &ReportFacts) -> String {
    if !record.right() {
        return PASS.to_owned();
    }
    record
        .composed
        .as_ref()
        .and_then(|c| c.get("verdict"))
        .and_then(Value::as_str)
        .map_or_else(|| old_verdict(record, criteria, report), str::to_owned)
}

async fn repo_with(sha: &str) -> Option<String> {
    let root = std::env::var_os(REPOS_ENV)?;
    let dirs = std::fs::read_dir(root).ok()?;
    for dir in dirs.flatten() {
        let repo = dir.path().to_string_lossy().into_owned();
        let spec = format!("{sha}^{{commit}}");
        if git(&repo, &["cat-file", "-e", &spec], None).await.is_some() {
            return Some(repo);
        }
    }
    None
}

async fn output_as_of(persist: &PersistHandle, record: &Labelled, command: &str) -> String {
    let row: Option<(Value,)> = sqlx::query_as(OUTPUT_AS_OF)
        .bind(command.trim())
        .bind(record.created_at)
        .fetch_optional(persist.pool())
        .await
        .ok()
        .flatten();
    row.map(|(content,)| excerpt(&content)).unwrap_or_default()
}

async fn ready(persist: &PersistHandle, mut record: Labelled) -> Option<Ready> {
    let criteria: Vec<CriterionText> =
        serde_json::from_value(record.state.get("criteria")?.clone()).ok()?;
    let mut report: ReportFacts =
        serde_json::from_value(record.state.get("report")?.clone()).ok()?;
    let expected = expected(&record, &criteria, &report);
    report.exercised_output = output_as_of(persist, &record, &report.exercised_command).await;
    let files = candidate_files(&report.exercised_command, &report.tests_changed);
    if let Some(repo) = repo_with(&report.commit).await {
        report.check_tests = test_names(&repo, &report.commit, &files).await;
    }
    record.state["report"] = serde_json::to_value(&report).ok()?;
    Some(Ready {
        record,
        criteria,
        report,
        expected,
    })
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let records = labelled::load(persist, POINT, since, limit).await?;
    let total = records.len();
    let mut out = Vec::with_capacity(total);
    for record in records {
        if let Some(item) = ready(persist, record).await {
            out.push(item);
        }
    }
    let skipped = total - out.len();
    Ok((out, skipped))
}

impl ReplayPoint for LeafJudge {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = labelled::SOURCE;

    fn item_id(item: &Ready) -> String {
        item.record.record_id.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.record.created_at
    }

    fn label(item: &Ready) -> String {
        format!("{} expected={}", item.record.label, item.expected)
    }

    fn prepare(item: &Ready) -> Prepared {
        Prepared {
            state: item.record.state.clone(),
            questions: questions(&item.criteria, &item.report),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let composed = decide(asked, &item.criteria, &item.report);
        let verdict = verdict_code(&composed);
        let right = verdict == item.expected;
        let score = match &composed {
            Composed::Pass { weakest } => *weakest,
            _ => 0.0,
        };
        Some(Judged {
            composed: json!({ "verdict": verdict, "expected": item.expected }),
            score,
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: Some(item.record.right() == right),
        })
    }

    fn question_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }

    fn current_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }
}
