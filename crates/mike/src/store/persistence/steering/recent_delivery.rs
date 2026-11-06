use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LastFire {
    pub payload_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_request_id: Option<i64>,
    pub pushed_at: Option<DateTime<Utc>>,
}

impl LastFire {
    pub fn was_delivered(&self) -> bool {
        self.delivered_request_id.is_some() || self.pushed_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SessionNote {
    pub steerer_id: i64,
    pub steerer_name: String,
    pub payload_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub outcome_label: Option<String>,
    pub reaction: Option<String>,
}

impl PersistHandle {
    pub async fn last_fire_for(
        &self,
        session_db_id: i64,
        steerer_id: i64,
    ) -> Result<Option<LastFire>, PersistError> {
        crate::metrics::time_db("last_fire_for", async move {
            let row = sqlx::query_as!(
                LastFire,
                "SELECT payload_text, created_at, delivered_request_id, pushed_at
                   FROM steering.interventions
                  WHERE session_id = $1 AND steerer_id = $2
                  ORDER BY created_at DESC
                  LIMIT 1",
                session_db_id,
                steerer_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn last_delivered_fire_since(
        &self,
        session_db_id: i64,
        steerer_id: i64,
        since: DateTime<Utc>,
    ) -> Result<Option<LastFire>, PersistError> {
        crate::metrics::time_db("last_delivered_fire_since", async move {
            let row = sqlx::query_as!(
                LastFire,
                "SELECT payload_text, created_at, delivered_request_id, pushed_at
                   FROM steering.interventions
                  WHERE session_id = $1 AND steerer_id = $2
                    AND (delivered_request_id IS NOT NULL OR pushed_at IS NOT NULL)
                    AND created_at >= $3
                  ORDER BY created_at DESC
                  LIMIT 1",
                session_db_id,
                steerer_id,
                since,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn session_notes_since(
        &self,
        session_db_id: i64,
        since: DateTime<Utc>,
        at: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<SessionNote>, PersistError> {
        crate::metrics::time_db("session_notes_since", async move {
            let rows = sqlx::query_as!(
                SessionNote,
                r#"SELECT i.steerer_id AS "steerer_id!", s.name AS "steerer_name!",
                          i.payload_text, i.created_at AS "created_at!",
                          CASE WHEN e.labelled_at <= $3 THEN e.outcome_label END
                              AS "outcome_label?",
                          CASE WHEN e.labelled_at <= $3 THEN e.label_evidence->>'reaction' END
                              AS "reaction?"
                     FROM steering.interventions i
                     JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
                     JOIN steering.steerers s ON s.id = i.steerer_id
                     LEFT JOIN steering.evaluations e ON e.intervention_uuid = i.uuid
                    WHERE i.session_id = $1
                      AND i.created_at >= $2
                      AND i.created_at <= $3
                      AND (i.reverted_at IS NULL OR i.reverted_at > $3)
                      AND (i.wake_state IS NULL
                           OR i.wake_state IN ('pending', 'delivering', 'pushed', 'appended'))
                      AND (i.delivered_request_id IS NOT NULL
                           OR i.pushed_at IS NOT NULL
                           OR i.expires_at > $3)
                    ORDER BY i.created_at DESC
                    LIMIT $4"#,
                session_db_id,
                since,
                at,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn declared_tool_names(&self, request_id: i64) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("declared_tool_names", async move {
            let names = sqlx::query_scalar!(
                "SELECT tool_name FROM requests.tools WHERE request_id = $1 ORDER BY ord",
                request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(names)
        })
        .await
    }
}
