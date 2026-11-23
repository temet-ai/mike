use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::types::RecipientActivity;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct RecipientActivityMemo(Memo<RecipientActivity>);

impl Default for RecipientActivityMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[],
            per_session: &[RowChangedTable::Requests],
        }))
    }
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn recipient_activity(
        self,
        recipients: &[(Uuid, i64)],
    ) -> Result<HashMap<i64, RecipientActivity>, PersistError> {
        let held = self.memo::<RecipientActivityMemo>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&held.0));
        let mut known = HashMap::new();
        let mut missing = Vec::new();
        for &(uuid, id) in recipients {
            match memo.as_ref().and_then(|memo| memo.get(uuid)) {
                Some(activity) => {
                    known.insert(id, activity);
                }
                None => missing.push((uuid, id)),
            }
        }
        if missing.is_empty() {
            return Ok(known);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let ids: Vec<i64> = missing.iter().map(|&(_, id)| id).collect();
        let last_request = self.latest_request_at_many(&ids).await?;
        let last_eligible = self.latest_eligible_turn_at_many(&ids).await?;
        for (uuid, id) in missing {
            let activity = RecipientActivity {
                last_request_at: last_request.get(&id).copied(),
                last_eligible_turn_at: last_eligible.get(&id).copied(),
            };
            if let (Some(memo), Some(ticket)) = (memo.as_ref(), ticket) {
                memo.remember(uuid, ticket, activity);
            }
            known.insert(id, activity);
        }
        Ok(known)
    }

    pub async fn fetch_recipient_activity(
        self,
        to_session_uuid: Uuid,
    ) -> Result<Option<RecipientActivity>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_recipient_activity", async move {
            let Some(session_id) = self.session_row_id(to_session_uuid).await? else {
                return Ok(None);
            };
            let mut activity = self
                .recipient_activity(&[(to_session_uuid, session_id)])
                .await?;
            Ok(Some(activity.remove(&session_id).unwrap_or_default()))
        })
        .await
    }

    async fn latest_eligible_turn_at_many(
        self,
        session_db_ids: &[i64],
    ) -> Result<HashMap<i64, DateTime<Utc>>, PersistError> {
        if session_db_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query!(
            r#"SELECT s.id AS "session_id!", l.created_at AS "created_at!"
                 FROM unnest($1::bigint[]) AS s(id)
                CROSS JOIN LATERAL (
                      SELECT r.created_at
                        FROM requests.requests r
                        JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                       WHERE r.session_id = s.id
                         AND ro.code IN ('primary', 'primary_first')
                       ORDER BY r.created_at DESC
                       LIMIT 1
                ) l"#,
            session_db_ids,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.session_id, r.created_at))
            .collect())
    }
}

#[cfg(test)]
#[path = "recipient_activity_tests.rs"]
mod tests;
