use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::guard::ChannelReachRow;

const DEFAULT_HEARTBEAT_SECS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Reach {
    pub stream_open: Option<bool>,
    pub channel_live: Option<bool>,
    pub checked_at: DateTime<Utc>,
}

pub fn stream_open(row: &ChannelReachRow, t: DateTime<Utc>) -> bool {
    let opened = row.stream_opened_at.is_some_and(|o| o <= t);
    let not_closed = row.stream_closed_at.is_none_or(|c| c > t);
    let interval = row
        .last_heartbeat_interval_seconds
        .map_or(DEFAULT_HEARTBEAT_SECS, i64::from);
    let fresh = row
        .last_seen_at
        .is_some_and(|seen| seen <= t && t - seen <= Duration::seconds(2 * interval));
    opened && not_closed && fresh
}

pub fn channel_live(session_uuid: Option<uuid::Uuid>) -> Option<bool> {
    session_uuid.map(|u| crate::mcp_channel::listeners().is_live(u))
}

pub async fn reach_at(
    persist: &PersistHandle,
    session_db_id: i64,
    session_uuid: Option<uuid::Uuid>,
    t: DateTime<Utc>,
    live: bool,
) -> Reach {
    let rows = persist
        .channel_reach_rows(session_db_id, session_uuid)
        .await;
    let stream = rows.ok().map(|rows| rows.iter().any(|r| stream_open(r, t)));
    Reach {
        stream_open: stream,
        channel_live: if live {
            channel_live(session_uuid)
        } else {
            None
        },
        checked_at: t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(opened: i64, closed: Option<i64>, seen: i64) -> ChannelReachRow {
        let base = Utc::now();
        ChannelReachRow {
            stream_opened_at: Some(base - Duration::seconds(opened)),
            stream_closed_at: closed.map(|c| base - Duration::seconds(c)),
            last_seen_at: Some(base - Duration::seconds(seen)),
            last_heartbeat_interval_seconds: Some(30),
        }
    }

    #[test]
    fn stream_open_needs_fresh_last_seen() {
        let now = Utc::now();
        assert!(stream_open(&row(600, None, 20), now));
        assert!(!stream_open(&row(600, None, 300), now));
    }

    #[test]
    fn closed_stream_is_not_open() {
        assert!(!stream_open(&row(600, Some(100), 20), Utc::now()));
    }

    #[test]
    fn channel_live_read_from_listeners() {
        assert_eq!(channel_live(None), None);
        assert_eq!(channel_live(Some(uuid::Uuid::new_v4())), Some(false));
    }
}
