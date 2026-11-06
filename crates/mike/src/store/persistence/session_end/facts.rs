use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SessionEndFacts {
    pub session_id: i64,
    pub end_turn_id: i64,
    pub end_turn_closed_at: Option<DateTime<Utc>>,
    pub next_state: String,
    pub session_class: String,
    pub session_class_rule: String,
    pub aborted: bool,
    pub interrupted: bool,
    pub close_cause: String,
    pub stop_reason: Option<String>,
    pub ended_on: String,
    pub genuine_turns: i64,
    pub prior_reactions: Vec<Option<String>>,
    pub launch_seat: Option<String>,
    pub launch_run_id: Option<String>,
    pub launch_template_id: Option<i64>,
    pub brief_path: Option<String>,
    pub brief_text: Option<String>,
    pub goal_role: Option<String>,
    pub all_genuine_are_brief: bool,
}

impl SessionEndFacts {
    pub fn settled(&self) -> bool {
        self.next_state == "none_session_settled"
    }
}

impl PersistHandle {
    pub async fn fetch_session_end_facts(
        &self,
        session_ids: &[i64],
        as_of: DateTime<Utc>,
    ) -> Result<Vec<SessionEndFacts>, PersistError> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("fetch_session_end_facts", async move {
            Ok(sqlx::query_file_as!(
                SessionEndFacts,
                "src/store/persistence/session_end/facts.sql",
                session_ids,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
