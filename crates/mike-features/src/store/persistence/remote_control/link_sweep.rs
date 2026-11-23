use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug, Clone)]
pub struct ChannelLinkSweepCandidate {
    pub session_uuid: Uuid,
    pub request_id: i64,
    pub request_body: Option<Vec<u8>>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn fetch_channel_link_sweep_candidates(
        self,
        since: DateTime<Utc>,
        limit: i64,
        scanned_request_ids: &[i64],
    ) -> Result<Vec<ChannelLinkSweepCandidate>, PersistError> {
        crate::metrics::time_db(
            "remote_control_fetch_channel_link_sweep_candidates",
            async move {
                let rows = sqlx::query!(
                    r#"
                SELECT s.session_uuid    AS "session_uuid!",
                       cand.id           AS "request_id!"
                  FROM sessions.sessions s
                  JOIN LATERAL (
                      SELECT rr.id
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

                let mut candidates = Vec::with_capacity(rows.len());
                for r in rows {
                    let request_body = if scanned_request_ids.contains(&r.request_id) {
                        None
                    } else {
                        rebuilt_body(&self.pool, r.request_id).await?
                    };
                    candidates.push(ChannelLinkSweepCandidate {
                        session_uuid: r.session_uuid,
                        request_id: r.request_id,
                        request_body,
                    });
                }
                Ok(candidates)
            },
        )
        .await
    }
}

/// The body as the parts store it: the envelope with its messages and tools put back.
async fn rebuilt_body(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> Result<Option<Vec<u8>>, PersistError> {
    let id = u64::try_from(request_id).unwrap_or_default();
    Ok(
        crate::store::persistence::rehydrate::fetch_request_body_by_id(pool, id)
            .await?
            .map(|served| served.bytes.to_vec()),
    )
}
