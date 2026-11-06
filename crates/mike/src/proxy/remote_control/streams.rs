use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use uuid::Uuid;

pub(crate) struct PendingFrame {
    pub(crate) bytes: Bytes,
    pub(crate) deadline: Instant,
    pub(crate) result: oneshot::Sender<bool>,
}

#[derive(Clone)]
struct Connection {
    token: Uuid,
    sender: mpsc::Sender<PendingFrame>,
}

#[derive(Clone, Default)]
pub(crate) struct ReplyStreams(Arc<DashMap<String, Connection>>);

pub(crate) struct Registration {
    streams: ReplyStreams,
    cse_id: String,
    token: Uuid,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.streams
            .0
            .remove_if(&self.cse_id, |_, entry| entry.token == self.token);
    }
}

impl ReplyStreams {
    pub(crate) fn register(&self, cse_id: String) -> (Registration, mpsc::Receiver<PendingFrame>) {
        let token = Uuid::new_v4();
        let (sender, receiver) = mpsc::channel(32);
        self.0.insert(cse_id.clone(), Connection { token, sender });
        (
            Registration {
                streams: self.clone(),
                cse_id,
                token,
            },
            receiver,
        )
    }

    pub(super) fn sender(&self, cse_id: &str) -> Option<mpsc::Sender<PendingFrame>> {
        self.0
            .get(cse_id)
            .filter(|entry| !entry.sender.is_closed())
            .map(|entry| entry.sender.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_close_does_not_remove_replacement() {
        let streams = ReplyStreams::default();
        let (old, _old_rx) = streams.register("cse_test".into());
        let (new, new_rx) = streams.register("cse_test".into());
        drop(old);
        assert!(streams.sender("cse_test").is_some());
        drop(new_rx);
        assert!(streams.sender("cse_test").is_none());
        drop(new);
        assert!(streams.0.is_empty());
    }
}
