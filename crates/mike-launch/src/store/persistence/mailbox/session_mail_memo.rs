use std::sync::Arc;

use uuid::Uuid;

use super::types::StoredMessageRow;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

pub struct SessionMail {
    pub inbox: Vec<StoredMessageRow>,
    pub outbox: Vec<StoredMessageRow>,
}

struct SessionMailMemo(Memo<Arc<SessionMail>>);

impl Default for SessionMailMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[],
            per_session: &[RowChangedTable::MailboxMessages],
        }))
    }
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn session_mail(
        self,
        session_uuid: Uuid,
        session_id: i64,
    ) -> Result<Arc<SessionMail>, PersistError> {
        let held = self.memo::<SessionMailMemo>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&held.0));
        if let Some(mail) = memo.as_ref().and_then(|memo| memo.get(session_uuid)) {
            return Ok(mail);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let mail = Arc::new(SessionMail {
            inbox: self.fetch_inbox_messages(session_id).await?,
            outbox: self.fetch_outbox_messages(session_id).await?,
        });
        if let (Some(memo), Some(ticket)) = (memo, ticket) {
            memo.remember(session_uuid, ticket, Arc::clone(&mail));
        }
        Ok(mail)
    }
}

#[cfg(test)]
#[path = "session_mail_memo_tests.rs"]
mod tests;
