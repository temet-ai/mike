pub mod config;
pub mod defaults;
pub mod live;
pub mod rejudge;
pub mod rubric;
pub mod run;
pub mod schema;
pub mod shadow;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndLabel {
    Done,
    ClosedInFrustration,
    CutOffNeutral,
    FleetSeat,
    Unclear,
}

impl SessionEndLabel {
    pub const ALL: [Self; 5] = [
        Self::Done,
        Self::ClosedInFrustration,
        Self::CutOffNeutral,
        Self::FleetSeat,
        Self::Unclear,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::ClosedInFrustration => "closed_in_frustration",
            Self::CutOffNeutral => "cut_off_neutral",
            Self::FleetSeat => "fleet_seat",
            Self::Unclear => "unclear",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|l| l.code() == raw.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EndRule {
    #[serde(rename = "fleet-seat-v1")]
    FleetSeatV1,
    #[serde(rename = "done-v1")]
    DoneV1,
}

impl EndRule {
    pub fn code(self) -> &'static str {
        match self {
            Self::FleetSeatV1 => "fleet-seat-v1",
            Self::DoneV1 => "done-v1",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "fleet-seat-v1" => Some(Self::FleetSeatV1),
            "done-v1" => Some(Self::DoneV1),
            _ => None,
        }
    }
}

pub const FAILURE_TAGS: [&str; 22] = [
    "misread-ask",
    "scope-creep",
    "stopped-short",
    "asked-instead-of-acting",
    "unverified-claim",
    "false-done",
    "ignored-instruction",
    "repeated-mistake",
    "wrong-approach",
    "overcomplicated",
    "slow-or-wasteful",
    "wrong-test-strategy",
    "destructive-action",
    "touched-others-work",
    "bad-explanation",
    "verbose-or-jargon",
    "tool-misuse",
    "wrong-model-or-agent-routing",
    "lost-context",
    "premature-handoff",
    "invented-facts",
    "style-violation",
];

pub fn normalise_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .take(3)
        .map(|t| {
            let t = t.trim().to_ascii_lowercase();
            if FAILURE_TAGS.contains(&t.as_str()) || t.starts_with("new:") {
                t
            } else {
                format!("new:{t}")
            }
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::{SessionEndLabel, normalise_tags};

    #[test]
    fn labels_parse_in_either_case_and_unknown_tags_get_the_new_prefix() {
        assert_eq!(
            SessionEndLabel::parse("CLOSED_IN_FRUSTRATION"),
            Some(SessionEndLabel::ClosedInFrustration)
        );
        assert_eq!(SessionEndLabel::parse("gave_up"), None);
        let tags = normalise_tags(&[
            "false-done".to_owned(),
            "Went Silent".to_owned(),
            "new:x".to_owned(),
            "extra".to_owned(),
        ]);
        assert_eq!(tags, vec!["false-done", "new:went silent", "new:x"]);
    }
}
