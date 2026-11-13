use super::ledger::Ledger;
use super::queue::LocalQueue;
use super::queue_replay;
use crate::base_state::BaseState;

pub fn spawn_queue_replay(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    state.tracker.clone().spawn(async move {
        let ledger = Ledger::new(Some(persist)).with_queue(LocalQueue::default_dir());
        let replay = queue_replay::replay(&ledger).await;
        if replay.replayed > 0 || !replay.stuck.is_empty() {
            tracing::info!(
                replayed = replay.replayed,
                stuck = replay.stuck.len(),
                "launch ledger queue replayed at proxy start"
            );
        }
    });
}
