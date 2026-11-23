use std::future::Future;
use std::marker::PhantomData;

use uuid::Uuid;

use super::PersistHandle;
use super::feed_memo::{Memo, Scope};
use crate::store::models::RowChangedTable;

pub struct GenerationMemo<V, Tag> {
    memo: Memo<V>,
    tag: PhantomData<fn() -> Tag>,
}

impl<V, Tag> Default for GenerationMemo<V, Tag> {
    fn default() -> Self {
        Self {
            memo: Memo::new(Scope {
                tables: &[RowChangedTable::ChangeGenerations],
                per_session: &[],
            }),
            tag: PhantomData,
        }
    }
}

impl PersistHandle {
    pub async fn kept_generation<Tag, V, E, Fut>(&self, read: impl FnOnce() -> Fut) -> Result<V, E>
    where
        Tag: 'static,
        V: Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<V, E>>,
    {
        self.kept_generation_while::<Tag, V, E, Fut>(read, |_| true)
            .await
    }

    pub async fn kept_generation_while<Tag, V, E, Fut>(
        &self,
        read: impl FnOnce() -> Fut,
        still: impl Fn(&V) -> bool,
    ) -> Result<V, E>
    where
        Tag: 'static,
        V: Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<V, E>>,
    {
        let slot = self.memo::<GenerationMemo<V, Tag>>();
        let bound = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&slot.memo));
        if let Some(kept) = bound.as_ref().and_then(|memo| memo.get(Uuid::nil()))
            && still(&kept)
        {
            return Ok(kept);
        }
        let ticket = bound.as_ref().map(|memo| memo.ticket());
        let value = read().await?;
        if let (Some(memo), Some(ticket)) = (bound, ticket) {
            memo.remember(Uuid::nil(), ticket, value.clone());
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "generation_memo_tests.rs"]
mod tests;
