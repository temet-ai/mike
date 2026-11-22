use std::collections::HashMap;

use dashmap::DashMap;
use uuid::Uuid;

use crate::store::models::RowChangedTable;
use crate::store::persistence::dossier_repo::{DossierDb, DossierPersist as _};
use crate::store::persistence::feed_memo::{Bound, Memo, Scope, Ticket};
use crate::store::persistence::{PersistError, PersistHandle};

const UUID_CAPACITY: usize = 8192;

pub(super) struct ClassifiedSessionsMemo {
    classified: Memo<String>,
    uuids: DashMap<i64, Uuid>,
}

impl Default for ClassifiedSessionsMemo {
    fn default() -> Self {
        Self {
            classified: Memo::new(Scope {
                tables: &[],
                per_session: &[RowChangedTable::Requests],
            }),
            uuids: DashMap::new(),
        }
    }
}

pub(super) struct Unchecked<'a> {
    pub(super) session_ids: Vec<i64>,
    uuids: HashMap<i64, Uuid>,
    ticket: Ticket,
    bound: Bound<'a, String>,
}

impl ClassifiedSessionsMemo {
    pub(super) async fn unchecked<'a>(
        &'a self,
        persist: &'a PersistHandle,
        session_ids: &[i64],
        classifier_version: &str,
    ) -> Result<Option<Unchecked<'a>>, PersistError> {
        let Some(bound) = persist.feed_memos.bound(&self.classified) else {
            return Ok(None);
        };
        let ticket = bound.ticket();
        let uuids = self.uuids_for(persist.dossier(), session_ids).await?;
        let session_ids = session_ids
            .iter()
            .copied()
            .filter(|id| {
                uuids
                    .get(id)
                    .is_none_or(|uuid| bound.get(*uuid).as_deref() != Some(classifier_version))
            })
            .collect();
        Ok(Some(Unchecked {
            session_ids,
            uuids,
            ticket,
            bound,
        }))
    }

    async fn uuids_for(
        &self,
        db: DossierDb<'_>,
        session_ids: &[i64],
    ) -> Result<HashMap<i64, Uuid>, PersistError> {
        let missing: Vec<i64> = session_ids
            .iter()
            .copied()
            .filter(|id| !self.uuids.contains_key(id))
            .collect();
        if !missing.is_empty() {
            if self.uuids.len() + missing.len() > UUID_CAPACITY {
                self.uuids.clear();
            }
            for (id, uuid) in db.session_uuids(&missing).await? {
                self.uuids.insert(id, uuid);
            }
        }
        Ok(session_ids
            .iter()
            .filter_map(|id| self.uuids.get(id).map(|uuid| (*id, *uuid)))
            .collect())
    }
}

impl Unchecked<'_> {
    pub(super) fn remember_classified(&self, pending_sessions: &[i64], classifier_version: &str) {
        for id in &self.session_ids {
            if pending_sessions.contains(id) {
                continue;
            }
            if let Some(uuid) = self.uuids.get(id) {
                self.bound
                    .remember(*uuid, self.ticket, classifier_version.to_owned());
            }
        }
    }
}

impl DossierDb<'_> {
    async fn session_uuids(self, ids: &[i64]) -> Result<Vec<(i64, Uuid)>, PersistError> {
        crate::metrics::time_db("dossier_session_uuids", async move {
            let rows = sqlx::query!(
                "SELECT id, session_uuid FROM sessions.sessions WHERE id = ANY($1)",
                ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.id, r.session_uuid)).collect())
        })
        .await
    }
}

#[cfg(test)]
#[path = "classify_memo_tests.rs"]
mod classify_memo_tests;
