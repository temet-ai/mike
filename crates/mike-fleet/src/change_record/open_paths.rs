use crate::launch::open_changes::{OpenChanges, PathsFuture};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub struct FleetOpenChanges;

async fn paths(persist: &PersistHandle, checkout: &str) -> Option<Vec<String>> {
    if !matches!(
        persist.fleet().newest_change_event(checkout).await,
        Ok(Some(_))
    ) {
        return None;
    }
    match persist.fleet().open_changes(checkout).await {
        Ok(rows) => Some(rows.into_iter().map(|r| r.path).collect()),
        Err(error) => {
            tracing::debug!(%error, "change record: open changes unavailable; committed left unknown");
            None
        }
    }
}

impl OpenChanges for FleetOpenChanges {
    fn paths<'a>(&'a self, persist: &'a PersistHandle, checkout: &'a str) -> PathsFuture<'a> {
        Box::pin(paths(persist, checkout))
    }
}
