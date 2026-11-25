use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::judge::{self, Judged};
use crate::llm::decisions::point::Decider;
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::code_answers::GroupFinding;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const KEEPS_PER_RULE: i64 = 3;
pub const REFUSALS: i32 = 2;
const REASON_MIN: usize = 15;

const GONE: &str = "No open finding of yours has this number. It may be fixed already.";
const TO_OPERATOR_ERROR: &str = "An error-level finding is not a session's to close. Your reason is with the operator and the finding stays open; fix it if you can.";
const TO_OPERATOR_KEEPS: &str = "This session has kept findings of this rule three times in 24 hours, so this one is with the operator. The finding stays open; fix it if you can.";
const TO_OPERATOR_REFUSED: &str = "Two answers were refused, so this one is with the operator. The finding stays open; fix it if you can.";
const TO_OPERATOR_UNJUDGED: &str =
    "The reason could not be judged now, so it is with the operator. The finding stays open.";
const ACCEPTED: &str = "Accepted and closed.";
const NOT_MINE: &str = "Recorded as not yours and closed. Leave the file alone.";
const TOO_SHORT: &str = "Give the reason: what about this code makes the rule wrong here.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Claim {
    Keep,
    NotMine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ruling {
    pub answer: &'static str,
    pub message: String,
    pub findings: usize,
    #[serde(skip)]
    pub ids: Vec<i64>,
}

impl Ruling {
    fn new(answer: &'static str, message: &str, findings: usize) -> Self {
        Self {
            answer,
            message: message.to_owned(),
            findings,
            ids: Vec::new(),
        }
    }
}

fn proof(finding: &GroupFinding) -> Option<String> {
    let at = finding
        .at
        .with_timezone(&chrono::Local)
        .format("%H:%M")
        .to_string();
    match finding.author.as_deref() {
        Some("tool_call") => Some(format!(
            "mike recorded this session's {} call writing {} at {at}",
            finding.tool_name.as_deref().unwrap_or("edit"),
            finding.path
        )),
        Some("shell") if finding.named => Some(format!(
            "mike recorded this session's shell command naming {} at {at}",
            finding.path
        )),
        Some("fleet_seat_trailer") => Some(format!(
            "this session's commit changed {} at {at}",
            finding.path
        )),
        _ => None,
    }
}

pub fn not_mine(group: &[GroupFinding]) -> Ruling {
    match group.iter().find_map(proof) {
        Some(proof) => Ruling::new("refused", &format!("Refused: {proof}."), group.len()),
        None => Ruling::new("not_mine", NOT_MINE, group.len()),
    }
}

pub fn before_judging(group: &[GroupFinding], keeps_today: i64) -> Option<Ruling> {
    let waiting = |why| Some(Ruling::new("waiting", why, group.len()));
    if group.iter().any(|f| f.severity == "error") {
        return waiting(TO_OPERATOR_ERROR);
    }
    if group.iter().any(|f| f.refusals >= REFUSALS) {
        return waiting(TO_OPERATOR_REFUSED);
    }
    if keeps_today >= KEEPS_PER_RULE {
        return waiting(TO_OPERATOR_KEEPS);
    }
    None
}

pub fn from_judge(judged: &Judged, findings: usize) -> Ruling {
    match judged {
        Judged::Accepted => Ruling::new("kept", ACCEPTED, findings),
        Judged::Refused(why) => Ruling::new("refused", &format!("Refused. {why}"), findings),
        Judged::Unanswered => Ruling::new("waiting", TO_OPERATOR_UNJUDGED, findings),
    }
}

pub async fn answer(
    persist: &PersistHandle,
    decider: Option<&Decider>,
    finding: i64,
    session: Uuid,
    claim: Claim,
    reason: &str,
) -> Result<Ruling, PersistError> {
    let reason = reason.trim();
    if claim == Claim::Keep && reason.chars().count() < REASON_MIN {
        return Ok(Ruling::new("refused", TOO_SHORT, 0));
    }
    let group = persist.fleet().code_answer_group(finding, session).await?;
    let Some(first) = group.first() else {
        return Ok(Ruling::new("gone", GONE, 0));
    };
    let mut ruling = match claim {
        Claim::NotMine => not_mine(&group),
        Claim::Keep => {
            let keeps = persist
                .fleet()
                .code_keeps_today(session, &first.rule_id)
                .await?;
            match (before_judging(&group, keeps), decider) {
                (Some(ruling), _) => ruling,
                (None, Some(decider)) => {
                    from_judge(&judge::judge(decider, &group, reason).await, group.len())
                }
                (None, None) => from_judge(&Judged::Unanswered, group.len()),
            }
        }
    };
    let ids: Vec<i64> = group.iter().map(|f| f.id).collect();
    persist
        .fleet()
        .record_code_answer(&ids, ruling.answer, reason, &ruling.message)
        .await?;
    ruling.ids = ids;
    Ok(ruling)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: &str, author: &str, named: bool) -> GroupFinding {
        GroupFinding {
            id: 1,
            line: 3,
            severity: severity.to_owned(),
            message: "m".to_owned(),
            matched: "x".to_owned(),
            checkout: "/c".to_owned(),
            path: "a.sh".to_owned(),
            rule_id: "r".to_owned(),
            refusals: 0,
            author: Some(author.to_owned()),
            tool_name: Some("Edit".to_owned()),
            named,
            at: chrono::Utc::now(),
        }
    }

    #[test]
    fn a_session_cannot_disown_a_change_mike_saw_it_make() {
        assert_eq!(
            not_mine(&[finding("warning", "tool_call", false)]).answer,
            "refused"
        );
        assert_eq!(
            not_mine(&[finding("warning", "shell", true)]).answer,
            "refused"
        );
        assert_eq!(
            not_mine(&[finding("warning", "shell", false)]).answer,
            "not_mine"
        );
    }

    #[test]
    fn errors_spent_keeps_and_repeat_refusals_go_to_the_operator() {
        let warning = [finding("warning", "tool_call", false)];
        assert_eq!(before_judging(&warning, 2), None);
        let answer = |ruling: Option<Ruling>| ruling.map(|r| r.answer);
        assert_eq!(answer(before_judging(&warning, 3)), Some("waiting"));
        let error = [finding("error", "tool_call", false)];
        assert_eq!(answer(before_judging(&error, 0)), Some("waiting"));
        let mut refused = finding("warning", "tool_call", false);
        refused.refusals = REFUSALS;
        assert_eq!(answer(before_judging(&[refused], 0)), Some("waiting"));
    }

    #[test]
    fn no_judge_never_means_accepted() {
        assert_eq!(from_judge(&Judged::Unanswered, 1).answer, "waiting");
    }
}
