pub mod candidates;
pub mod drift;
pub mod embedders;
pub mod floors;
pub mod items;
pub mod retrieval;
pub mod tagging;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use candidates::{CandidateDecision, CandidateMemberRow, NewCandidate, PatternCandidateRow};
pub use drift::{DriftRow, DriftSums, NewDriftRow};
pub use embedders::{EmbedderRow, NewEmbedder, PruneReport, VerifySample};
pub use floors::{FloorRow, FloorSample, FloorStats, NewFloor};
pub use items::{DirectiveSource, ItemCount, NewItem, SemanticItemRow, TurnSource};
pub use retrieval::{
    AnchorVector, CandidateRow, CandidateSearch, FacetVector, ScopeFilter, TaggerBacktestCase,
};
pub use tagging::{
    AnchorCandidateRow, AssignmentFilter, AssignmentRow, CorrectionForTagging, NewAssignment,
    ResidueRow,
};

pub fn parse_vector(text: &str) -> Vec<f32> {
    text.trim_matches(['[', ']'])
        .split(',')
        .filter_map(|x| x.trim().parse::<f32>().ok())
        .collect()
}

pub fn vector_literal(v: &[f32]) -> String {
    crate::store::persistence::operator_turns::vector_to_text_literal(v)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticWindow {
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub after_turn: i64,
    pub limit: i64,
}

#[cfg(test)]
mod tests {
    use super::{parse_vector, vector_literal};

    #[test]
    fn a_vector_round_trips_through_its_text_literal() {
        let v = vec![0.25_f32, -1.5, 3.0];
        assert_eq!(parse_vector(&vector_literal(&v)), v);
        assert!(parse_vector("[]").is_empty());
    }
}
