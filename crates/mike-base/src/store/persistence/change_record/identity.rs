use dashmap::DashMap;
use uuid::Uuid;

use crate::store::models::RowChangedTable;
use crate::store::persistence::feed_memo::{Memo, Scope};
use crate::store::persistence::{PersistError, PersistHandle};

const ID_CAPACITY: usize = 50_000;

/// Session identities kept in memory. A session's id and uuid never change once
/// written, so that pairing is kept for good. Its name, cwd and project can
/// change, and every change sends the session's `sessions` row_changed notice
/// (the sessions trigger for cwd and project, the session_goals trigger for the
/// name), which drops the kept identity. Without a running change feed every
/// lookup reads the database, as before.
struct Identities {
    by_uuid: Memo<SessionIdentity>,
    uuid_of: DashMap<i64, Uuid>,
}

impl Default for Identities {
    fn default() -> Self {
        Self {
            by_uuid: Memo::new(Scope {
                tables: &[],
                per_session: &[RowChangedTable::Sessions],
            }),
            uuid_of: DashMap::new(),
        }
    }
}

impl Identities {
    fn note_id(&self, identity: &SessionIdentity) {
        if self.uuid_of.len() >= ID_CAPACITY {
            self.uuid_of.clear();
        }
        self.uuid_of.insert(identity.id, identity.uuid);
    }
}

impl PersistHandle {
    pub async fn session_identities(
        &self,
        ids: &[i64],
    ) -> Result<Vec<SessionIdentity>, PersistError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let identities = self.memo::<Identities>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and_then(|_| self.feed_memos.bound(&identities.by_uuid));
        let mut found = Vec::with_capacity(ids.len());
        let mut missing = Vec::new();
        for id in ids {
            let kept = memo.as_ref().and_then(|memo| {
                let uuid = *identities.uuid_of.get(id)?;
                memo.get(uuid)
            });
            match kept {
                Some(identity) => found.push(identity),
                None => missing.push(*id),
            }
        }
        if !missing.is_empty() {
            let ticket = memo.as_ref().map(|memo| memo.ticket());
            for identity in self.read_session_identities(&missing).await? {
                identities.note_id(&identity);
                if let (Some(memo), Some(ticket)) = (memo.as_ref(), ticket) {
                    memo.remember(identity.uuid, ticket, identity.clone());
                }
                found.push(identity);
            }
        }
        found.sort_by_key(|identity| identity.id);
        found.dedup_by_key(|identity| identity.id);
        Ok(found)
    }

    pub async fn session_identity_by_uuid(
        &self,
        uuid: Uuid,
    ) -> Result<Option<SessionIdentity>, PersistError> {
        let identities = self.memo::<Identities>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and_then(|_| self.feed_memos.bound(&identities.by_uuid));
        if let Some(identity) = memo.as_ref().and_then(|memo| memo.get(uuid)) {
            return Ok(Some(identity));
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let row = self.read_session_identity_by_uuid(uuid).await?;
        if let Some(identity) = &row {
            identities.note_id(identity);
            if let (Some(memo), Some(ticket)) = (memo.as_ref(), ticket) {
                memo.remember(uuid, ticket, identity.clone());
            }
        }
        Ok(row)
    }

    async fn read_session_identities(
        &self,
        ids: &[i64],
    ) -> Result<Vec<SessionIdentity>, PersistError> {
        crate::metrics::time_db("change_record_session_identities", async move {
            let rows = sqlx::query_file_as!(
                SessionIdentity,
                "src/store/persistence/change_record/session_identities.sql",
                ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    async fn read_session_identity_by_uuid(
        &self,
        uuid: Uuid,
    ) -> Result<Option<SessionIdentity>, PersistError> {
        crate::metrics::time_db("change_record_session_identity_by_uuid", async move {
            let row = sqlx::query_file_as!(
                SessionIdentity,
                "src/store/persistence/change_record/session_identity_by_uuid.sql",
                uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionIdentity {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub project_id: i64,
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
