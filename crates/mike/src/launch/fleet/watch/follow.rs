use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::{POLL, Toggles, Tree, record};
use crate::change_record::watch::FileDelta;
use crate::store::persistence::PersistHandle;

pub(super) struct FleetTree<'a> {
    pub(super) persist: &'a PersistHandle,
    pub(super) toggles: &'a Toggles,
    pub(super) run: &'a str,
    pub(super) repo: &'a str,
}

pub(super) async fn follow_feed(
    fleet: &FleetTree<'_>,
    tree: &mut Tree,
    mut feed: broadcast::Receiver<Vec<FileDelta>>,
    token: &CancellationToken,
) {
    tracing::info!(run = %fleet.run, repo = %fleet.repo, "fleet: taking the checkout's changes from the change record");
    record(fleet.persist, fleet.run, &tree.observe(fleet.repo).await).await;
    while next_feed_step(fleet, tree, &mut feed, token).await {}
}

async fn next_feed_step(
    fleet: &FleetTree<'_>,
    tree: &mut Tree,
    feed: &mut broadcast::Receiver<Vec<FileDelta>>,
    token: &CancellationToken,
) -> bool {
    tokio::select! {
        () = token.cancelled() => false,
        received = feed.recv() => take_from_feed(fleet, tree, received).await,
        () = tokio::time::sleep(POLL) => fleet.toggles.load().change_record,
    }
}

async fn take_from_feed(
    fleet: &FleetTree<'_>,
    tree: &mut Tree,
    received: Result<Vec<FileDelta>, broadcast::error::RecvError>,
) -> bool {
    let changes = match received {
        Ok(deltas) => tree.apply_feed(&deltas),
        Err(broadcast::error::RecvError::Lagged(_)) => {
            tree.forget_status();
            tree.observe(fleet.repo).await
        }
        Err(broadcast::error::RecvError::Closed) => return false,
    };
    record(fleet.persist, fleet.run, &changes).await;
    true
}
