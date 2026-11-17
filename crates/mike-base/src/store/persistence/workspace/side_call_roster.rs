use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

pub const SIDE_CALL_SOURCE: &str = "side-call";

#[derive(Debug, Clone)]
pub struct SideCallRoster<'a> {
    pub session_id: i64,
    pub read_through: DateTime<Utc>,
    pub goal: &'a str,
    pub next_steps: &'a str,
    pub display_name: &'a str,
}

impl PersistHandle {
    pub async fn write_side_call_roster(
        &self,
        roster: SideCallRoster<'_>,
    ) -> Result<bool, PersistError> {
        let result = sqlx::query!(
            r#"INSERT INTO sessions.session_goals
                   (session_id, goal, next_steps, set_by, display_name, goal_source, updated_at)
               VALUES ($1, $2, $3, $5, NULLIF($4, ''), $5, now())
               ON CONFLICT (session_id) DO UPDATE
                   SET goal         = CASE WHEN btrim(sessions.session_goals.goal) = ''
                                             OR sessions.session_goals.goal_source = $5
                                           THEN excluded.goal
                                           ELSE sessions.session_goals.goal END,
                       goal_source  = CASE WHEN btrim(sessions.session_goals.goal) = ''
                                             OR sessions.session_goals.goal_source = $5
                                           THEN $5
                                           ELSE sessions.session_goals.goal_source END,
                       next_steps   = excluded.next_steps,
                       display_name = COALESCE(NULLIF(sessions.session_goals.display_name, ''),
                                               excluded.display_name),
                       set_by       = $5,
                       updated_at   = now()
                 WHERE sessions.session_goals.updated_at < $6"#,
            roster.session_id,
            roster.goal,
            roster.next_steps,
            roster.display_name,
            SIDE_CALL_SOURCE,
            roster.read_through,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
