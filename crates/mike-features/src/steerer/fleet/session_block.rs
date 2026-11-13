use chrono::{DateTime, Duration, Utc};

use crate::dossier::render::steerer::render_steerer;
use crate::dossier::view::{AsOf, Needs, SteererSection};
use crate::store::persistence::PersistHandle;

pub const SESSION_BLOCK_CHARS: usize = 8_000;

pub fn session_sections() -> [SteererSection; 4] {
    [
        SteererSection::EarlierNotes,
        SteererSection::TurnOpening,
        SteererSection::EarlierTurns,
        SteererSection::RecentReactions,
    ]
}

pub struct BlockRequest {
    pub session_db_id: i64,
    pub turn_id: i64,
    pub as_of: AsOf,
    pub notes_window: Duration,
}

impl BlockRequest {
    pub fn now(session_db_id: i64, turn_id: i64, notes_window: Duration) -> Self {
        Self::at(session_db_id, turn_id, notes_window, Utc::now())
    }

    pub fn at(
        session_db_id: i64,
        turn_id: i64,
        notes_window: Duration,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_db_id,
            turn_id,
            as_of: AsOf::knowledge(at),
            notes_window,
        }
    }
}

pub async fn session_block(persist: &PersistHandle, request: &BlockRequest) -> String {
    let sections = session_sections();
    let needs = Needs::for_sections(Some(request.turn_id), &sections, Some(request.notes_window));
    match crate::dossier::view::load(persist, request.session_db_id, request.as_of, needs).await {
        Ok(inputs) => {
            render_steerer(
                &inputs,
                Some(request.turn_id),
                &sections,
                SESSION_BLOCK_CHARS,
            )
            .allocated
            .text
        }
        Err(error) => {
            tracing::warn!(
                session_db_id = request.session_db_id,
                turn_id = request.turn_id,
                %error,
                "steerer fleet: the session's dossier could not be read; the judge sees no earlier notes or reactions"
            );
            String::new()
        }
    }
}

pub fn with_session_block(message: String, block: &str) -> String {
    if block.is_empty() {
        message
    } else {
        format!("{message}\n\n{block}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_appended_after_the_judge_message_and_only_when_there_is_one() {
        assert_eq!(with_session_block("ask".to_owned(), ""), "ask");
        assert_eq!(
            with_session_block("ask".to_owned(), "NOTES"),
            "ask\n\nNOTES"
        );
    }

    #[test]
    fn the_block_loads_only_what_its_sections_read() {
        let needs = Needs::for_sections(Some(7), &session_sections(), Some(Duration::minutes(60)));
        assert!(needs.stream && needs.turns && needs.verdicts);
        assert_eq!(needs.notes_window, Some(Duration::minutes(60)));
        assert!(!needs.entries && !needs.ledger && !needs.goals && !needs.labels);
        assert_eq!(needs.open_turn, None);
        assert_eq!(needs.steering_facts, None);
        assert_eq!(needs.semantic, None);
    }
}
