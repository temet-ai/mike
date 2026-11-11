use super::SimilarTurnDto;
use crate::api::detail::Detail;
use serde::Serialize;

pub(crate) const POINTER_PROMPT_CHARS: usize = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum TurnEntry {
    Full(Box<SimilarTurnDto>),
    Lead(LeadTurn),
    Pointer(TurnPointer),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LeadTurn {
    pub(crate) turn_id: i64,
    pub(crate) reference: String,
    pub(crate) session_id: String,
    pub(crate) created_at: String,
    pub(crate) user_prompt: String,
    pub(crate) final_assistant_text: String,
    pub(crate) outcome: String,
}

impl LeadTurn {
    fn of(turn: &SimilarTurnDto) -> Self {
        Self {
            turn_id: turn.turn.turn_id,
            reference: format!("turn:{}", turn.turn.turn_id),
            session_id: turn.turn.session_id.clone(),
            created_at: turn.turn.created_at.clone(),
            user_prompt: turn.turn.user_prompt.clone(),
            final_assistant_text: turn.turn.final_assistant_text.clone(),
            outcome: turn.turn.outcome.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TurnPointer {
    pub(crate) turn_id: i64,
    pub(crate) reference: String,
    pub(crate) created_at: String,
    pub(crate) session_id: String,
    pub(crate) prompt: String,
}

impl TurnPointer {
    fn of(turn: &SimilarTurnDto) -> Self {
        Self {
            turn_id: turn.turn.turn_id,
            reference: format!("turn:{}", turn.turn.turn_id),
            created_at: turn.turn.created_at.clone(),
            session_id: turn.turn.session_id.clone(),
            prompt: turn
                .turn
                .user_prompt
                .chars()
                .take(POINTER_PROMPT_CHARS)
                .collect(),
        }
    }
}

pub(crate) const HOW_TO_GET_THE_REST: &str = "Read one with read_project_knowledge and its reference, or repeat the recall with detail=full to get every turn in full. Cite the reference you use.";

pub(crate) fn more(turns: &[SimilarTurnDto], detail: Detail) -> Option<serde_json::Value> {
    let pointers = turns.len().saturating_sub(1);
    (detail.is_best() && !turns.is_empty()).then(|| {
        serde_json::json!({
            "shown_in_full": 1,
            "pointers": pointers,
            "how": HOW_TO_GET_THE_REST,
        })
    })
}

pub(crate) fn entries(turns: &[SimilarTurnDto], detail: Detail) -> Vec<TurnEntry> {
    turns
        .iter()
        .enumerate()
        .map(|(position, turn)| match (detail, position) {
            (Detail::Best, 0) => TurnEntry::Lead(LeadTurn::of(turn)),
            (Detail::Best, _) => TurnEntry::Pointer(TurnPointer::of(turn)),
            _ => TurnEntry::Full(Box::new(turn.clone())),
        })
        .collect()
}
