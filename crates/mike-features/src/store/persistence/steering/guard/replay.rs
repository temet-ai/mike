use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayWindow {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub after_turn: i64,
    pub limit: i64,
    pub turn_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct GuardReplayTurn {
    pub turn_id: i64,
    pub session_id: i64,
    pub session_uuid: uuid::Uuid,
    pub closed_at: Option<DateTime<Utc>>,
    pub opened_by: Option<String>,
    pub user_prompt: Option<String>,
    pub agent_id: Option<String>,
    pub tail: String,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn guard_replay_turns(
        self,
        w: &ReplayWindow,
    ) -> Result<Vec<GuardReplayTurn>, PersistError> {
        crate::metrics::time_db("guard_replay_turns", async move {
            Ok(if w.turn_ids.is_empty() {
                sqlx::query_file_as!(
                    GuardReplayTurn,
                    "src/store/persistence/steering/guard/replay_turns.sql",
                    w.since,
                    w.until,
                    w.after_turn,
                    w.limit,
                )
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_file_as!(
                    GuardReplayTurn,
                    "src/store/persistence/steering/guard/replay_turns_for_ids.sql",
                    w.since,
                    w.until,
                    w.after_turn,
                    w.limit,
                    &w.turn_ids,
                )
                .fetch_all(&self.pool)
                .await?
            })
        })
        .await
    }
}
