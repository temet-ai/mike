use uuid::Uuid;

use super::EnrolmentRow;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct EnrolmentMemo(Memo<Vec<EnrolmentRow>>);

impl Default for EnrolmentMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[],
            per_session: &[RowChangedTable::SteeringEnrolments],
        }))
    }
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn list_active_enrolments_for_session(
        self,
        session_uuid: Uuid,
    ) -> Result<Vec<EnrolmentRow>, PersistError> {
        let held = self.memo::<EnrolmentMemo>();
        let memo = self.feed_memos.bound(&held.0);
        if let Some(rows) = memo.as_ref().and_then(|memo| memo.get(session_uuid)) {
            return Ok(rows);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let rows = self
            .read_active_enrolments_for_session(session_uuid)
            .await?;
        if let (Some(memo), Some(ticket)) = (memo, ticket) {
            memo.remember(session_uuid, ticket, rows.clone());
        }
        Ok(rows)
    }

    pub(super) fn enrolments_changed(self, session_uuid: Uuid) {
        self.feed_memos
            .record(RowChangedTable::SteeringEnrolments, Some(session_uuid));
    }

    pub(super) fn forget_all_enrolments(self) {
        self.memo::<EnrolmentMemo>().0.forget_all();
    }
}

#[cfg(test)]
#[path = "enrolment_memo_tests.rs"]
mod enrolment_memo_tests;
