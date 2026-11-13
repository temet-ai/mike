use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug, Clone)]
pub struct ChannelLinkSweepCandidate {
    pub session_uuid: Uuid,
    pub request_body: Option<Vec<u8>>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn fetch_channel_link_sweep_candidates(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<ChannelLinkSweepCandidate>, PersistError> {
        crate::metrics::time_db(
            "remote_control_fetch_channel_link_sweep_candidates",
            async move {
                let rows = sqlx::query!(
                    r#"
                SELECT s.session_uuid    AS "session_uuid!",
                       cand.request_body AS "request_body?"
                  FROM sessions.sessions s
                  JOIN LATERAL (
                      SELECT rr.request_body
                        FROM requests.requests rr
                       WHERE rr.session_id = s.id
                       ORDER BY rr.created_at DESC, rr.id DESC
                       LIMIT 1
                  ) cand ON true
                 WHERE s.last_seen >= $1
                   AND NOT EXISTS (
                       SELECT 1 FROM remote_control.channels c
                        WHERE c.local_session_uuid = s.session_uuid
                   )
                 ORDER BY s.last_seen DESC
                 LIMIT $2
                "#,
                    since,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|r| ChannelLinkSweepCandidate {
                        session_uuid: r.session_uuid,
                        request_body: r.request_body,
                    })
                    .collect())
            },
        )
        .await
    }
}
