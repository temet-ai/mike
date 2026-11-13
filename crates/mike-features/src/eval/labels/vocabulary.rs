use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Turn,
    SessionEnd,
    Evaluation,
    Intervention,
}

impl SubjectKind {
    pub fn code(self) -> &'static str {
        match self {
            SubjectKind::Turn => "turn",
            SubjectKind::SessionEnd => "session_end",
            SubjectKind::Evaluation => "evaluation",
            SubjectKind::Intervention => "intervention",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Gold,
    Relayed,
    AutoShadow,
    Seed,
}

impl Tier {
    pub fn code(self) -> &'static str {
        match self {
            Tier::Gold => "gold",
            Tier::Relayed => "relayed",
            Tier::AutoShadow => "auto_shadow",
            Tier::Seed => "seed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    OperatorClick,
    OperatorConfirmedRelay,
    RelayedQuote,
    NextMessageAuto,
    DoubleBlindAdjudicated,
    SingleModel,
}

impl Method {
    pub fn code(self) -> &'static str {
        match self {
            Method::OperatorClick => "operator_click",
            Method::OperatorConfirmedRelay => "operator_confirmed_relay",
            Method::RelayedQuote => "relayed_quote",
            Method::NextMessageAuto => "next_message_auto",
            Method::DoubleBlindAdjudicated => "double_blind_adjudicated",
            Method::SingleModel => "single_model",
        }
    }

    pub fn tier(self) -> Tier {
        match self {
            Method::OperatorClick | Method::OperatorConfirmedRelay => Tier::Gold,
            Method::RelayedQuote => Tier::Relayed,
            Method::NextMessageAuto => Tier::AutoShadow,
            Method::DoubleBlindAdjudicated | Method::SingleModel => Tier::Seed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Reaction,
    Delivery,
    SessionEnd,
    FailureTag,
    FactOwnPromptCorrectsPrior,
    FactAssistantSelfCorrection,
    FactIrreversibleAction,
    SteerValid,
}

impl Axis {
    pub fn code(self) -> &'static str {
        match self {
            Axis::Reaction => "reaction",
            Axis::Delivery => "delivery",
            Axis::SessionEnd => "session_end",
            Axis::FailureTag => "failure_tag",
            Axis::FactOwnPromptCorrectsPrior => "fact_own_prompt_corrects_prior",
            Axis::FactAssistantSelfCorrection => "fact_assistant_self_correction",
            Axis::FactIrreversibleAction => "fact_irreversible_action",
            Axis::SteerValid => "steer_valid",
        }
    }

    pub fn operator_writable(self) -> bool {
        matches!(self, Axis::Reaction | Axis::Delivery | Axis::SessionEnd)
    }

    pub fn subject_kind(self) -> Option<SubjectKind> {
        match self {
            Axis::Reaction | Axis::Delivery => Some(SubjectKind::Turn),
            Axis::SessionEnd => Some(SubjectKind::SessionEnd),
            _ => None,
        }
    }
}

pub const REACTION_LABELS: [&str; 5] = [
    "corrected",
    "redirected",
    "accepted",
    "no_response",
    "unclear",
];
pub const DELIVERY_LABELS: [&str; 6] = [
    "shipped",
    "delivered",
    "partial",
    "none",
    "question",
    "not_applicable",
];
pub const SESSION_END_LABELS: [&str; 5] = [
    "done",
    "closed_in_frustration",
    "cut_off_neutral",
    "fleet_seat",
    "unclear",
];

pub fn labels_for(axis: Axis) -> &'static [&'static str] {
    match axis {
        Axis::Reaction => &REACTION_LABELS,
        Axis::Delivery => &DELIVERY_LABELS,
        Axis::SessionEnd => &SESSION_END_LABELS,
        _ => &[],
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn every_method_maps_to_its_tier() {
        assert_eq!(Method::OperatorClick.tier(), Tier::Gold);
        assert_eq!(Method::RelayedQuote.tier(), Tier::Relayed);
        assert_eq!(Method::SingleModel.tier(), Tier::Seed);
    }

    #[test]
    fn only_three_axes_are_operator_writable() {
        assert!(Axis::Reaction.operator_writable());
        assert!(!Axis::FailureTag.operator_writable());
        assert_eq!(labels_for(Axis::Delivery).len(), 6);
    }
}
