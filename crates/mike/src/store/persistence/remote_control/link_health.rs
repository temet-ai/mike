use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelLinkCounts {
    pub channels: i64,
    pub linked: i64,
}

impl PersistHandle {
    pub async fn recent_channel_link_counts(&self) -> Result<ChannelLinkCounts, PersistError> {
        crate::metrics::time_db("remote_control_recent_channel_link_counts", async move {
            let row = sqlx::query!(
                r#"SELECT count(*) AS "channels!", count(local_session_uuid) AS "linked!"
                     FROM remote_control.channels
                    WHERE first_seen_at >= now() - interval '24 hours'
                      AND first_seen_at < now() - interval '15 minutes'"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(ChannelLinkCounts {
                channels: row.channels,
                linked: row.linked,
            })
        })
        .await
    }
}
