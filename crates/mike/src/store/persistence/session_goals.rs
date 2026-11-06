use super::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub struct SessionGoal {
    pub goal: String,
    pub next_steps: String,
    pub role: Option<String>,
    pub display_name: Option<String>,
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct SetSessionIdentity<'a> {
    pub session_uuid: uuid::Uuid,
    pub display_name: Option<&'a str>,
    pub role: Option<&'a str>,
    pub goal: Option<&'a str>,
    pub next_steps: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub set_by: &'a str,
}

impl PersistHandle {
    pub async fn set_session_goal(
        &self,
        identity: SetSessionIdentity<'_>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("set_session_goal", async move {
            let result = sqlx::query!(
                r#"INSERT INTO sessions.session_goals
                       (session_id, goal, next_steps, set_by, role, display_name, instructions, updated_at)
                   SELECT s.id, COALESCE($2, ''), COALESCE($3, ''), $4, $5, $6, $7, now()
                     FROM sessions.sessions s
                    WHERE s.session_uuid = $1
                   ON CONFLICT (session_id) DO UPDATE
                       SET goal         = CASE WHEN $2::text IS NOT NULL THEN $2 ELSE sessions.session_goals.goal END,
                           next_steps   = CASE WHEN $3::text IS NOT NULL THEN $3 ELSE sessions.session_goals.next_steps END,
                           set_by       = $4,
                           role         = COALESCE($5, sessions.session_goals.role),
                           display_name = COALESCE($6, sessions.session_goals.display_name),
                           instructions = COALESCE($7, sessions.session_goals.instructions),
                           updated_at   = now()"#,
                identity.session_uuid,
                identity.goal,
                identity.next_steps,
                identity.set_by,
                identity.role,
                identity.display_name,
                identity.instructions,
            )
            .execute(&self.pool)
            .await?;

            if result.rows_affected() == 0 {
                return Err(PersistError::DataInvariant(format!(
                    "set_session_goal: session_uuid {} not found",
                    identity.session_uuid
                )));
            }
            Ok(())
        })
        .await
    }

    pub async fn get_session_goal(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<SessionGoal>, PersistError> {
        crate::metrics::time_db("get_session_goal", async move {
            let row = sqlx::query!(
                r#"SELECT sg.goal, sg.next_steps, sg.role, sg.display_name, sg.instructions
                     FROM sessions.session_goals sg
                     JOIN sessions.sessions s ON s.id = sg.session_id
                    WHERE s.session_uuid = $1"#,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| SessionGoal {
                goal: r.goal,
                next_steps: r.next_steps,
                role: r.role,
                display_name: r.display_name,
                instructions: r.instructions,
            }))
        })
        .await
    }
}
