use serde_json::{Value, json};
use tokio::sync::broadcast;

const CAPACITY: usize = 32;
const MAX_EVENT_BYTES: usize = 256 * 1024;

pub(crate) struct EventReceiver(broadcast::Receiver<Value>);
pub(crate) struct EventSender(broadcast::Sender<Value>);

pub(crate) fn channel() -> (EventSender, EventReceiver) {
    let (sender, receiver) = broadcast::channel(CAPACITY);
    (EventSender(sender), EventReceiver(receiver))
}

impl EventSender {
    pub(crate) fn send(&self, event: Value) {
        let event = if event.to_string().len() > MAX_EVENT_BYTES {
            gap(1, "notification exceeds 256 KiB")
        } else {
            event
        };
        let _ = self.0.send(event);
    }

    pub(crate) fn gap(&self, reason: &str) {
        self.send(gap(1, reason));
    }
}

impl EventReceiver {
    pub(crate) async fn recv(&mut self) -> Option<Value> {
        match self.0.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(count)) => {
                Some(gap(count, "notification consumer lagged"))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

fn gap(count: u64, reason: &str) -> Value {
    json!({"method":"mike/observationGap","params":{"dropped":count,"reason":reason}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn slow_consumer_observes_bounded_queue_gap() -> anyhow::Result<()> {
        let (sender, mut receiver) = channel();
        for n in 0..CAPACITY + 3 {
            sender.send(json!({"method":"turn/completed","params":{"n":n}}));
        }
        let event = receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))?;
        assert_eq!(event["method"], "mike/observationGap");
        assert_eq!(event["params"]["dropped"], 3);
        let first = receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))?;
        assert_eq!(first["params"]["n"], 3);
        Ok(())
    }

    #[tokio::test]
    async fn oversized_event_reports_gap_without_retaining_payload() -> anyhow::Result<()> {
        let (sender, mut receiver) = channel();
        sender.send(json!({"text":"a".repeat(MAX_EVENT_BYTES)}));
        let event = receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))?;
        assert_eq!(event["method"], "mike/observationGap");
        assert!(event.to_string().len() < 200);
        Ok(())
    }
}
