mod contexts;
mod insert;
pub(crate) use insert::insert_turn_verdict_on;
mod reads;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use contexts::JudgeTurnContextRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictMode {
    Live,
    Shadow,
    Eval,
}

impl VerdictMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Shadow => "shadow",
            Self::Eval => "eval",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "live" => Some(Self::Live),
            "shadow" => Some(Self::Shadow),
            "eval" => Some(Self::Eval),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VerdictCallUsage {
    pub generation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerdictShape<'a> {
    TwoAxis {
        reaction: &'a str,
        delivery: &'a str,
        failure_tags: Option<Vec<String>>,
        severity: Option<&'a str>,
    },
    Legacy {
        label: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerdictJudgement<'a> {
    pub shape: VerdictShape<'a>,
    pub confidence: f32,
    pub rationale: &'a str,
    pub unmet_ask: &'a str,
    pub evidence_quality: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnVerdictInsert<'a> {
    pub turn_id: i64,
    pub mode: VerdictMode,
    pub bundle_id: Option<i64>,
    pub rubric_version: &'a str,
    pub input_builder_version: &'a str,
    pub slices: Vec<String>,
    pub model: &'a str,
    pub judgement: VerdictJudgement<'a>,
    pub next_genuine_request_id: Option<i64>,
    pub as_of: DateTime<Utc>,
    pub input_text: &'a str,
    pub usage: VerdictCallUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyStamp<'a> {
    pub outcome_code: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct TurnVerdictRow {
    pub id: i64,
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub session_uuid: uuid::Uuid,
    pub turn_outcome: Option<String>,
    pub mode: String,
    pub bundle_id: Option<i64>,
    pub rubric_version: String,
    pub input_builder_version: String,
    pub slices: Vec<String>,
    pub model: String,
    pub source: String,
    pub reaction: Option<String>,
    pub delivery: Option<String>,
    pub legacy_label: Option<String>,
    pub confidence: f32,
    pub rationale: String,
    pub unmet_ask: String,
    pub evidence_quality: String,
    pub projected: bool,
    pub next_genuine_request_id: Option<i64>,
    pub as_of: DateTime<Utc>,
    pub input_text: String,
    pub generation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub failure_tags: Option<Vec<String>>,
    pub severity: Option<String>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnVerdictFilter {
    pub mode: Option<VerdictMode>,
    pub bundle_id: Option<i64>,
    pub rubric_version: Option<String>,
    pub session_uuid: Option<uuid::Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

pub(crate) const VERDICT_COLUMNS: &str = "v.id, v.operator_turn_id, v.session_id, s.session_uuid, \
     o.code AS turn_outcome, v.mode, v.bundle_id, v.rubric_version, v.input_builder_version, \
     v.slices, v.model, v.source, v.reaction, v.delivery, v.legacy_label, v.confidence, \
     v.rationale, v.unmet_ask, v.evidence_quality, v.projected, v.next_genuine_request_id, \
     v.as_of, v.input_text, v.generation_id, v.cost_usd, v.failure_tags, v.severity, \
     v.prompt_tokens, v.completion_tokens, v.latency_ms, v.created_at";

pub(crate) const VERDICT_FROM: &str = "FROM memory.turn_verdicts v \
     JOIN sessions.sessions s ON s.id = v.session_id \
     JOIN memory.operator_turns t ON t.id = v.operator_turn_id \
     LEFT JOIN memory.turn_outcomes o ON o.id = t.outcome_id";

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 4] = [
        include_str!("mod.rs"),
        include_str!("insert.rs"),
        include_str!("reads.rs"),
        include_str!("contexts.rs"),
    ];

    #[test]
    fn no_statement_in_the_module_updates_or_deletes_a_verdict() {
        let update = ["UPDATE", "memory.turn_verdicts"].join(" ");
        let delete = ["DELETE FROM", "memory.turn_verdicts"].join(" ");
        for source in SOURCES {
            assert!(!source.contains(&update));
            assert!(!source.contains(&delete));
        }
    }

    #[test]
    fn verdict_modes_round_trip_through_their_database_codes() {
        use super::VerdictMode;
        for mode in [VerdictMode::Live, VerdictMode::Shadow, VerdictMode::Eval] {
            assert_eq!(VerdictMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(VerdictMode::parse("projected"), None);
    }
}
