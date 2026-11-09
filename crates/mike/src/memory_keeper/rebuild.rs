use std::collections::BTreeMap;

use super::cards::render;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::work_threads::WorkThreadError;
use crate::store::persistence::work_threads::cards::{DirtyThread, NewCard};

const BATCH_THREADS: usize = 50;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PassReport {
    pub(crate) rebuilt: usize,
    pub(crate) written: usize,
}

pub(crate) fn merge(dirty: impl IntoIterator<Item = DirtyThread>) -> Vec<DirtyThread> {
    let mut newest: BTreeMap<i64, DirtyThread> = BTreeMap::new();
    for thread in dirty {
        newest
            .entry(thread.thread_id)
            .and_modify(|kept| kept.observed_at = kept.observed_at.max(thread.observed_at))
            .or_insert(thread);
    }
    newest.into_values().collect()
}

async fn rebuild_batch(
    persist: &PersistHandle,
    batch: &[DirtyThread],
) -> Result<PassReport, WorkThreadError> {
    let ids: Vec<i64> = batch.iter().map(|dirty| dirty.thread_id).collect();
    let observed: BTreeMap<i64, DirtyThread> = batch
        .iter()
        .map(|dirty| (dirty.thread_id, *dirty))
        .collect();
    let cards: Vec<NewCard> = persist
        .work_thread_card_inputs(&ids)
        .await?
        .iter()
        .filter_map(|inputs| {
            observed
                .get(&inputs.thread.id)
                .map(|dirty| render(inputs).into_new_card(*dirty))
        })
        .collect();
    let written = persist.write_work_thread_cards(&cards).await?;
    Ok(PassReport {
        rebuilt: cards.len(),
        written,
    })
}

pub(crate) async fn rebuild(
    persist: &PersistHandle,
    dirty: &[DirtyThread],
) -> Result<PassReport, WorkThreadError> {
    let mut report = PassReport::default();
    for batch in dirty.chunks(BATCH_THREADS) {
        let done = rebuild_batch(persist, batch).await?;
        report.rebuilt += done.rebuilt;
        report.written += done.written;
    }
    Ok(report)
}
