use tokio::sync::broadcast::error::RecvError;

use super::phrase_point;
use crate::base_state::BaseState;
use crate::store::models::ProxyEvent;

pub fn spawn_phrase_filter(base: &BaseState) {
    let mut rx = base.event_tx.subscribe();
    let base = base.clone();
    let shutdown = base.shutdown();
    let tracker = base.tracker.clone();
    base.tracker.clone().spawn(async move {
        loop {
            let received = tokio::select! {
                () = shutdown.cancelled() => return,
                received = rx.recv() => received,
            };
            match received {
                Ok(ProxyEvent::DiagnosticReady {
                    id,
                    count,
                    accumulated_text,
                }) if count > 0 => {
                    let Some(env) = base.decision_env() else {
                        continue;
                    };
                    let store = base.store.clone();
                    tracker.spawn(async move {
                        phrase_point::filter_stored(&env, &store, id, &accumulated_text).await;
                    });
                }
                Err(RecvError::Closed) => return,
                Ok(_) | Err(RecvError::Lagged(_)) => {}
            }
        }
    });
    tracing::info!("diagnostic phrase filter: spawned");
}
