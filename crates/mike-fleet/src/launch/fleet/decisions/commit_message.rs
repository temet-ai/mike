use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::config::{Mode, Point};
use super::{FleetDecisions, text};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;

pub const QUESTION_SET: &str = "fleet_commit_message_v1";
pub const DIFF_CAP_BYTES: usize = 8 * 1024;
pub const DESCRIBES_BELOW: f64 = 0.4;
pub const UNMENTIONED_ABOVE: f64 = 0.6;

const DESCRIBES: &str = "message_describes_diff";
const UNMENTIONED: &str = "diff_has_unmentioned_work";

pub struct Landed<'a> {
    pub repo: &'a str,
    pub sha: &'a str,
    pub message: &'a str,
    pub seat: &'a str,
    pub session_db_id: Option<i64>,
}

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            DESCRIBES.to_owned(),
            noul(
                "Does the commit message describe what the diff changes?",
                "The message names the change the diff makes.",
                "The message describes something the diff does not do, or says nothing about what it does.",
            ),
        ),
        (
            UNMENTIONED.to_owned(),
            noul(
                "Does the diff contain a substantial change that the commit message does not mention?",
                "The diff holds a real change, such as another feature, fix or file group, that the message leaves out.",
                "Everything substantial in the diff is covered by the message.",
            ),
        ),
    ])
}

pub fn state(message: &str, numstat: &str, diff: &str) -> Value {
    let files: Vec<Value> = numstat
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (added, deleted, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some(json!({"path": path, "added": added, "deleted": deleted}))
        })
        .collect();
    json!({
        "commit_message": message.trim(),
        "files": files,
        "unified_diff": text::head(diff, DIFF_CAP_BYTES),
        "unified_diff_cut": diff.len() > DIFF_CAP_BYTES,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub warnings: Vec<String>,
    pub score: f64,
}

pub fn compose(asked: &Asked) -> Verdict {
    if asked.failed() {
        return Verdict {
            warnings: Vec::new(),
            score: 1.0,
        };
    }
    let describes = asked.noul(DESCRIBES).unwrap_or(1.0);
    let unmentioned = asked.noul(UNMENTIONED).unwrap_or(0.0);
    let mut warnings = Vec::new();
    if describes < DESCRIBES_BELOW {
        warnings.push(format!(
            "the commit message does not seem to describe the change it contains (match {:.0}%); say what the diff does",
            describes * 100.0
        ));
    }
    if unmentioned > UNMENTIONED_ABOVE {
        warnings.push(format!(
            "the diff holds work the commit message does not mention ({:.0}% sure); name it in the message or commit it separately",
            unmentioned * 100.0
        ));
    }
    let score = if warnings.is_empty() {
        describes.min(1.0 - unmentioned)
    } else {
        1.0 - describes.min(1.0 - unmentioned)
    };
    Verdict { warnings, score }
}

pub async fn for_report(
    decisions: Option<&FleetDecisions>,
    (def, seated): (
        &super::super::definition::FleetDefinition,
        &super::super::seated::Seated,
    ),
    (request, report): (
        &super::super::commit::CommitRequest,
        &super::super::commit::CommitReport,
    ),
) -> Vec<String> {
    let Some(sha) = report.sha.as_deref().filter(|_| report.refused.is_none()) else {
        return Vec::new();
    };
    let mode = def.session_decision_mode(Point::CommitMessage, seated, request.session);
    let session_db_id = match decisions {
        Some(d) => d
            .persist()
            .resolve_session_db_id(&request.session.to_string())
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let landed = Landed {
        repo: &def.repo,
        sha,
        message: &request.message,
        seat: &report.seat,
        session_db_id,
    };
    warnings(decisions, mode, &landed).await
}

async fn read_diff(landed: &Landed<'_>) -> Option<(String, String)> {
    let parent = format!("{}^", landed.sha);
    let numstat = super::super::watch::git(
        landed.repo,
        &["diff", "--numstat", &parent, landed.sha],
        None,
    )
    .await?;
    let diff = super::super::watch::git(landed.repo, &["diff", &parent, landed.sha], None).await?;
    Some((
        String::from_utf8_lossy(&numstat).into_owned(),
        String::from_utf8_lossy(&diff).into_owned(),
    ))
}

pub async fn warnings(
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    landed: &Landed<'_>,
) -> Vec<String> {
    let Some(decider) = decisions.and_then(|d| d.decider(Point::CommitMessage, mode)) else {
        return Vec::new();
    };
    let Some((numstat, diff)) = read_diff(landed).await else {
        return Vec::new();
    };
    ask(&decider, landed, state(landed.message, &numstat, &diff)).await
}

async fn ask(decider: &Decider, landed: &Landed<'_>, state: Value) -> Vec<String> {
    let subject = DecisionSubject {
        session_id: landed.session_db_id,
        seat: Some(landed.seat.to_owned()),
        commit_sha: Some(landed.sha.to_owned()),
        ..DecisionSubject::default()
    };
    let thresholds =
        json!({"describes_below": DESCRIBES_BELOW, "unmentioned_above": UNMENTIONED_ABOVE});
    let asked = decider
        .ask(QUESTION_SET, state, questions(), subject, thresholds)
        .await;
    let verdict = compose(&asked);
    if !decider.applies() {
        decider
            .mark(
                &asked,
                asked.composed(json!({"score": verdict.score})),
                "none",
            )
            .await;
        return Vec::new();
    }
    let applied = if verdict.warnings.is_empty() {
        "none"
    } else {
        "warned"
    };
    decider
        .mark(
            &asked,
            asked.composed(json!({"score": verdict.score})),
            applied,
        )
        .await;
    verdict.warnings
}

#[cfg(test)]
#[path = "commit_message_tests.rs"]
mod tests;
