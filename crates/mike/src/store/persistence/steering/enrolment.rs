use super::{EnrolmentRow, SessionEnrolmentRow};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn fetch_session_enrolments(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Vec<SessionEnrolmentRow>, PersistError> {
        let rows = sqlx::query_as!(
            SessionEnrolmentRow,
            r#"SELECT
                   enrolment_id,
                   session_id,
                   session_uuid,
                   steerer_id,
                   steerer_name,
                   steerer_display_name,
                   steerer_enabled,
                   mode,
                   authority,
                   enrolled_at,
                   enrolled_by,
                   disenrolled_at
               FROM mike.session_enrolments_v
              WHERE session_uuid = $1
              ORDER BY enrolled_at DESC"#,
            session_uuid,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub(crate) async fn enrol_auto_steerers(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("enrol_auto_steerers", async move {
            let result = sqlx::query!(
                r#"INSERT INTO steering.enrolments
                       (session_id, steerer_id, mode_id, authority_id, enrolled_by)
                   SELECT s.id, st.id, st.default_mode_id, st.authority_id, 'auto'
                     FROM sessions.sessions   s
                    CROSS JOIN steering.steerers st
                    WHERE s.session_uuid = $1
                      AND st.auto_enrol  = true
                      AND st.enabled     = true
                      AND st.deleted_at IS NULL
                      AND (st.project_id IS NULL OR st.project_id = s.project_id)
                   ON CONFLICT (session_id, steerer_id) DO NOTHING"#,
                session_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub(crate) async fn insert_enrolment(
        &self,
        session_uuid: uuid::Uuid,
        steerer_id: i64,
        mode_code: &str,
        authority_code: &str,
        enrolled_by: &str,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_enrolment", async move {
            let mut tx = self.pool.begin().await?;
            let row = sqlx::query!(
                r#"INSERT INTO steering.enrolments
                       (session_id, steerer_id, mode_id, authority_id, enrolled_by)
                   SELECT s.id, $2, m.id, a.id, $5
                     FROM sessions.sessions    s,
                          steering.modes       m,
                          steering.authorities a
                    WHERE s.session_uuid = $1
                      AND m.code         = $3
                      AND a.code         = $4
                   RETURNING id"#,
                session_uuid,
                steerer_id,
                mode_code,
                authority_code,
                enrolled_by,
            )
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(row.id)
        })
        .await
    }

    pub(crate) async fn delete_enrolment(&self, enrolment_id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_enrolment", async move {
            let result = sqlx::query!(
                "UPDATE steering.enrolments
                    SET disenrolled_at = NOW()
                  WHERE id              = $1
                    AND disenrolled_at IS NULL",
                enrolment_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub(crate) async fn list_active_enrolments_for_session(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Vec<EnrolmentRow>, PersistError> {
        crate::metrics::time_db("list_active_enrolments_for_session", async move {
            let rows = sqlx::query_as!(
                EnrolmentRow,
                r#"SELECT
                       e.id            AS "id!",
                       e.session_id    AS "session_id!",
                       e.steerer_id    AS "steerer_id!",
                       m.code          AS "mode!",
                       a.code          AS "authority!",
                       e.enrolled_at   AS "enrolled_at!",
                       e.enrolled_by   AS "enrolled_by!",
                       e.disenrolled_at
                   FROM steering.enrolments e
                   JOIN steering.modes       m ON m.id = e.mode_id
                   JOIN steering.authorities a ON a.id = e.authority_id
                   JOIN sessions.sessions    s ON s.id = e.session_id
                  WHERE s.session_uuid   = $1
                    AND e.disenrolled_at IS NULL
                  ORDER BY e.id ASC"#,
                session_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn count_active_enrolments_all(
        &self,
    ) -> Result<std::collections::HashMap<i64, i64>, PersistError> {
        crate::metrics::time_db("count_active_enrolments_all", async move {
            let rows = sqlx::query!(
                r#"SELECT steerer_id, COUNT(*)::BIGINT AS "active_enrolment_count!"
                     FROM steering.enrolments
                    WHERE disenrolled_at IS NULL
                    GROUP BY steerer_id"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.steerer_id, r.active_enrolment_count))
                .collect())
        })
        .await
    }
}
