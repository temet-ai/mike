use serde::Serialize;

use crate::store::persistence::steering::promotion::ArmRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Bad,
    Good,
    Missing(&'static str),
}

pub fn scored_outcome(row: &ArmRow, end_label: Option<&str>) -> Outcome {
    if row.scored_turn_id.is_none() {
        return Outcome::Missing("no_carrier");
    }
    if row.scored_is_last && end_label == Some("closed_in_frustration") {
        return Outcome::Bad;
    }
    match row.scored_reaction.as_deref() {
        Some("corrected") => Outcome::Bad,
        Some("accepted" | "redirected") => Outcome::Good,
        Some("no_response") => match end_label {
            Some("done" | "cut_off_neutral" | "fleet_seat") => Outcome::Good,
            _ => Outcome::Missing("no_response_without_end_label"),
        },
        Some("unclear") => Outcome::Missing("unclear"),
        _ => Outcome::Missing("no_live_verdict"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(scored: Option<i64>, reaction: Option<&str>, last: bool) -> ArmRow {
        ArmRow {
            id: 1,
            uuid: uuid::Uuid::nil(),
            session_id: 1,
            operator_turn_id: 10,
            question: Some("relapse".to_owned()),
            arm: Some("treated".to_owned()),
            holdback_draw: Some(0.5),
            arm_anchor_turn_id: Some(10),
            due_at: None,
            reach_at_due: None,
            dispatched_at: chrono::Utc::now(),
            outcome_label: None,
            label_source: None,
            carrier_request_id: scored.map(|_| 99),
            scored_turn_id: scored,
            scored_reaction: reaction.map(str::to_owned),
            scored_is_last: last,
            delivered: true,
            wake_state: None,
        }
    }

    #[test]
    fn scored_outcome_follows_the_reaction_on_the_scored_turn() {
        assert_eq!(
            scored_outcome(&row(Some(11), Some("corrected"), false), None),
            Outcome::Bad
        );
        assert_eq!(
            scored_outcome(&row(Some(11), Some("accepted"), false), None),
            Outcome::Good
        );
        assert_eq!(
            scored_outcome(&row(None, None, false), None),
            Outcome::Missing("no_carrier")
        );
        assert_eq!(
            scored_outcome(&row(Some(11), None, true), Some("closed_in_frustration")),
            Outcome::Bad
        );
        assert_eq!(
            scored_outcome(&row(Some(11), Some("no_response"), true), Some("done")),
            Outcome::Good
        );
        assert_eq!(
            scored_outcome(&row(Some(11), Some("unclear"), false), None),
            Outcome::Missing("unclear")
        );
    }
}
