use chrono::{DateTime, Utc};

use super::PersistError;
pub use super::workspace::side_call_roster::{SIDE_CALL_SOURCE, SideCallRoster};

#[derive(Debug, Clone, Copy)]
pub struct RosterWindow {
    pub lookback_hours: i32,
    pub settle_secs: i32,
    pub min_gap_minutes: i32,
    pub limit: i64,
}

#[derive(Debug, Clone)]
pub struct RosterCandidate {
    pub session_id: i64,
    pub session_uuid: uuid::Uuid,
    pub title: Option<String>,
    pub last_closed_at: DateTime<Utc>,
    pub goal: Option<String>,
    pub next_steps: Option<String>,
    pub display_name: Option<String>,
    pub goal_source: Option<String>,
}

impl RosterCandidate {
    pub fn goal_is_ours(&self) -> bool {
        let empty = self.goal.as_deref().is_none_or(|g| g.trim().is_empty());
        empty || self.goal_source.as_deref() == Some(SIDE_CALL_SOURCE)
    }
}

#[derive(Debug, Clone)]
pub struct RecentTurn {
    pub closed_at: DateTime<Utc>,
    pub user_prompt: String,
    pub final_text: String,
    pub files_touched: Vec<String>,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn roster_candidates(
        self,
        window: RosterWindow,
    ) -> Result<Vec<RosterCandidate>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/roster_candidates.sql",
            window.lookback_hours,
            window.settle_secs,
            window.min_gap_minutes,
            window.limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RosterCandidate {
                session_id: r.session_id,
                session_uuid: r.session_uuid,
                title: r.title,
                last_closed_at: r.last_closed_at,
                goal: r.goal,
                next_steps: r.next_steps,
                display_name: r.display_name,
                goal_source: r.goal_source,
            })
            .collect())
    }

    pub async fn roster_recent_turns(
        self,
        session_id: i64,
        turns: i64,
        max_chars: i32,
    ) -> Result<Vec<RecentTurn>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/roster_recent_turns.sql",
            session_id,
            turns,
            max_chars,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .rev()
            .map(|r| RecentTurn {
                closed_at: r.closed_at,
                user_prompt: r.user_prompt,
                final_text: r.final_text,
                files_touched: r.files_touched,
            })
            .collect())
    }
}
