use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::models::RowChangedTable;
use crate::store::persistence::feed_memo::{Memo, Scope};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::RosterScope;

pub(super) const MEMBER_TABLES: &[RowChangedTable] = &[
    RowChangedTable::WorkspaceSessionRegistrations,
    RowChangedTable::RemoteControlChannels,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RosterChannel {
    pub(super) id: i64,
    pub(super) cse_id: String,
    pub(super) client_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RosterMember {
    pub(super) session_id: i64,
    pub(super) session_uuid: Uuid,
    pub(super) cwd: String,
    pub(super) worktree: Option<String>,
    pub(super) registered_via: String,
    pub(super) registered_at: DateTime<Utc>,
    pub(super) released_at: Option<DateTime<Utc>>,
    pub(super) channels: Vec<RosterChannel>,
}

struct MemberRow {
    session_id: i64,
    session_uuid: Uuid,
    cwd: String,
    worktree: Option<String>,
    registered_via: String,
    registered_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
    channel_id: Option<i64>,
    cse_id: Option<String>,
    client_version: Option<String>,
}

impl MemberRow {
    fn channel(&self) -> Option<RosterChannel> {
        Some(RosterChannel {
            id: self.channel_id?,
            cse_id: self.cse_id.clone()?,
            client_version: self.client_version.clone(),
        })
    }

    fn into_member(self) -> RosterMember {
        let channels = self.channel().into_iter().collect();
        RosterMember {
            session_id: self.session_id,
            session_uuid: self.session_uuid,
            cwd: self.cwd,
            worktree: self.worktree,
            registered_via: self.registered_via,
            registered_at: self.registered_at,
            released_at: self.released_at,
            channels,
        }
    }
}

fn group(rows: Vec<MemberRow>) -> Vec<RosterMember> {
    let mut members: Vec<RosterMember> = Vec::new();
    for row in rows {
        match members.last_mut() {
            Some(last) if last.session_id == row.session_id => {
                last.channels.extend(row.channel());
            }
            _ => members.push(row.into_member()),
        }
    }
    members
}

pub(super) fn includes_released(scope: RosterScope) -> bool {
    scope == RosterScope::All
}

fn memo_key(project_id: i64, released: bool) -> Uuid {
    Uuid::from_u64_pair(project_id.cast_unsigned(), u64::from(released))
}

struct RosterMembers(Memo<Arc<Vec<RosterMember>>>);

impl Default for RosterMembers {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: MEMBER_TABLES,
            per_session: &[],
        }))
    }
}

impl PersistHandle {
    pub(super) async fn roster_members(
        &self,
        project_id: i64,
        released: bool,
    ) -> Result<Arc<Vec<RosterMember>>, PersistError> {
        let slot = self.memo::<RosterMembers>();
        let bound = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&slot.0));
        let key = memo_key(project_id, released);
        if let Some(kept) = bound.as_ref().and_then(|memo| memo.get(key)) {
            return Ok(kept);
        }
        let ticket = bound.as_ref().map(|memo| memo.ticket());
        let members = Arc::new(self.read_roster_members(project_id, released).await?);
        if let (Some(memo), Some(ticket)) = (bound, ticket) {
            memo.remember(key, ticket, Arc::clone(&members));
        }
        Ok(members)
    }

    async fn read_roster_members(
        &self,
        project_id: i64,
        released: bool,
    ) -> Result<Vec<RosterMember>, PersistError> {
        crate::metrics::time_db("workspace_fetch_roster_members", async move {
            let rows = sqlx::query_as!(
                MemberRow,
                r#"SELECT r.session_id       AS "session_id!",
                          s.session_uuid     AS "session_uuid!",
                          r.cwd              AS "cwd!",
                          r.worktree         AS "worktree?",
                          r.registered_via   AS "registered_via!",
                          r.registered_at    AS "registered_at!",
                          r.released_at      AS "released_at?",
                          c.id               AS "channel_id?",
                          c.cse_id           AS "cse_id?",
                          c.client_version   AS "client_version?"
                     FROM workspace.session_registrations r
                     JOIN sessions.sessions s ON s.id = r.session_id
                     LEFT JOIN remote_control.channels c ON c.local_session_uuid = s.session_uuid
                    WHERE r.project_id = $1
                      AND ($2 OR r.released_at IS NULL)
                    ORDER BY r.registered_at DESC, r.session_id, c.id"#,
                project_id,
                released,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(group(rows))
        })
        .await
    }

    pub fn registrations_changed(&self, session: Option<Uuid>) {
        self.feed_memos
            .record(RowChangedTable::WorkspaceSessionRegistrations, session);
    }
}

#[cfg(test)]
#[path = "members_tests.rs"]
mod members_tests;
