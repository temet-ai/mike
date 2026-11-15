use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::bar_evidence::{Evidence, LabelRow, evidence_for, label_rows};
use super::bar_gh::{IssueTracker, MILESTONE};
use super::{BAR_MIN_LABELLED, BAR_SHARE, OVERDUE, PointScore, WINDOW};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Created,
    Commented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BarIssue {
    pub point: String,
    pub issue: u32,
    pub action: Action,
    pub at: DateTime<Utc>,
}

pub(crate) struct Context {
    pub run: i64,
    pub now: DateTime<Utc>,
}

pub(crate) fn title(point: &str) -> String {
    format!(
        "Jev decision point {point} under {:.0}% right",
        BAR_SHARE * 100.0
    )
}

fn percent(right: usize, labelled: usize) -> f64 {
    if labelled == 0 {
        return 0.0;
    }
    right as f64 * 100.0 / labelled as f64
}

fn body(score: &PointScore, evidence: &Evidence, ctx: &Context, update: bool) -> String {
    let point = &score.counts.point;
    let lead = if update {
        format!("Update from decisions loop run {}.", ctx.run)
    } else {
        format!("Filed by decisions loop run {}.", ctx.run)
    };
    let sources = evidence
        .sources
        .iter()
        .map(|s| {
            format!(
                "- {}: {}/{} right ({:.0}%)",
                s.source,
                s.right,
                s.labelled,
                percent(s.right, s.labelled)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let wrong = evidence
        .newest_wrong
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{lead}\n\nDecision point `{point}` was right on {} of {} labelled outcomes ({:.0}%) in the {} days up to {}. The bar is {:.0}% right on at least {} labelled outcomes.\n\nBy label source:\n{sources}\n\nNewest wrong decisions.records ids: {wrong}\n\nThe point stays on; the decisions loop changes no mode. A person decides what to do.",
        score.right,
        score.labelled,
        percent(score.right, score.labelled),
        WINDOW.num_days(),
        ctx.now.to_rfc3339(),
        BAR_SHARE * 100.0,
        BAR_MIN_LABELLED,
    )
}

fn filed_recently(previous: &[BarIssue], point: &str, now: DateTime<Utc>) -> bool {
    previous
        .iter()
        .any(|entry| entry.point == point && now - entry.at < OVERDUE)
}

async fn open_milestone(tracker: &dyn IssueTracker) -> Option<&'static str> {
    match tracker.milestone_open(MILESTONE).await {
        Ok(true) => Some(MILESTONE),
        Ok(false) => None,
        Err(error) => {
            tracing::warn!(%error, "decisions_loop: could not list milestones; filing without one");
            None
        }
    }
}

async fn file_one(
    tracker: &dyn IssueTracker,
    score: &PointScore,
    evidence: &Evidence,
    ctx: &Context,
) -> Result<BarIssue, String> {
    let point = score.counts.point.clone();
    let title = title(&point);
    let (issue, action) = match tracker.open_issue(&title).await? {
        Some(number) => {
            tracker
                .comment(number, &body(score, evidence, ctx, true))
                .await?;
            (number, Action::Commented)
        }
        None => {
            let milestone = open_milestone(tracker).await;
            let number = tracker
                .create(&title, &body(score, evidence, ctx, false), milestone)
                .await?;
            (number, Action::Created)
        }
    };
    Ok(BarIssue {
        point,
        issue,
        action,
        at: ctx.now,
    })
}

pub(crate) async fn file(
    tracker: &dyn IssueTracker,
    under: &[&PointScore],
    rows: &[LabelRow],
    previous: &[BarIssue],
    ctx: &Context,
) -> Vec<BarIssue> {
    let mut filed = Vec::new();
    for score in under {
        let point = score.counts.point.as_str();
        if filed_recently(previous, point, ctx.now) {
            continue;
        }
        match file_one(tracker, score, &evidence_for(point, rows), ctx).await {
            Ok(entry) => filed.push(entry),
            Err(error) => {
                tracing::warn!(%error, point, "decisions_loop: could not file the under-the-bar issue");
            }
        }
    }
    filed
}

async fn recent_filed(
    persist: &PersistHandle,
    since: DateTime<Utc>,
) -> Result<Vec<BarIssue>, crate::store::persistence::PersistError> {
    let reports = crate::llm::decisions::store(persist)
        .decision_loop_reports_since(since, "bar_issues")
        .await?;
    Ok(reports
        .into_iter()
        .filter_map(|value| serde_json::from_value::<Vec<BarIssue>>(value).ok())
        .flatten()
        .collect())
}

pub(crate) async fn file_under_bar(
    persist: &PersistHandle,
    tracker: &dyn IssueTracker,
    scores: &[PointScore],
    ctx: &Context,
) -> Vec<BarIssue> {
    let under: Vec<&PointScore> = scores.iter().filter(|s| s.under_bar()).collect();
    if under.is_empty() {
        return Vec::new();
    }
    let points: Vec<String> = under.iter().map(|s| s.counts.point.clone()).collect();
    let previous = recent_filed(persist, ctx.now - OVERDUE)
        .await
        .map_err(|error| error.to_string());
    let rows = label_rows(persist, ctx.now - WINDOW, &points)
        .await
        .map_err(|error| error.to_string());
    match (previous, rows) {
        (Ok(previous), Ok(rows)) => file(tracker, &under, &rows, &previous, ctx).await,
        (Err(error), _) | (_, Err(error)) => {
            tracing::warn!(%error, "decisions_loop: could not read the figures for the under-the-bar issues");
            Vec::new()
        }
    }
}

pub(crate) async fn store_report(persist: &PersistHandle, run: i64, report: &Value) {
    let stored = crate::llm::decisions::store(persist)
        .update_decision_loop_report(run, report)
        .await;
    if let Err(error) = stored {
        tracing::warn!(%error, run, "decisions_loop: could not record the filed issues on the run");
    }
}
