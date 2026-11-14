use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use super::prelabel::Prelabel;
use crate::store::persistence::PersistError;
use crate::store::persistence::eval_runs::{EvalRunItemRow, RunItemsFilter, RunItemsPage};

const BATCH: i64 = 500;
const MAX_ROWS: i64 = 10_000;

pub fn parse(value: Option<&str>) -> Result<bool, String> {
    match value.map(str::trim) {
        None | Some("") => Ok(false),
        Some("uncertain") => Ok(true),
        Some(other) => Err(format!("review must be uncertain, got {other:?}")),
    }
}

pub fn needs_review(turn_id: i64, prelabels: &BTreeMap<i64, Prelabel>) -> bool {
    prelabels.get(&turn_id).is_none_or(|p| p.uncertain)
}

pub fn window<T>(items: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    items
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(0))
        .take(usize::try_from(limit).unwrap_or(0))
        .collect()
}

pub trait PageReader: Sync {
    fn read<'a>(
        &'a self,
        scan: &'a RunItemsFilter,
    ) -> Pin<Box<dyn Future<Output = Result<RunItemsPage, PersistError>> + Send + 'a>>;
}

pub async fn uncertain_page<L>(
    filter: &RunItemsFilter,
    reader: &dyn PageReader,
    prelabels_for: L,
) -> Result<RunItemsPage, PersistError>
where
    L: Fn(&[i64]) -> BTreeMap<i64, Prelabel>,
{
    let mut scan = RunItemsFilter {
        limit: BATCH,
        offset: 0,
        ..filter.clone()
    };
    let mut rows: Vec<EvalRunItemRow> = Vec::new();
    let mut reviewed = 0;
    loop {
        let page = reader.read(&scan).await?;
        if scan.offset == 0 {
            reviewed = page.reviewed;
        }
        let got = i64::try_from(page.items.len()).unwrap_or(BATCH);
        rows.extend(page.items);
        scan.offset += got;
        if got < BATCH || scan.offset >= page.total.min(MAX_ROWS) {
            break;
        }
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.operator_turn_id).collect();
    let prelabels = prelabels_for(&ids);
    rows.retain(|r| needs_review(r.operator_turn_id, &prelabels));
    let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    Ok(RunItemsPage {
        items: window(rows, filter.limit, filter.offset),
        total,
        reviewed,
    })
}

#[cfg(test)]
#[path = "review_queue_tests.rs"]
mod tests;
