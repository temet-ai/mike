use serde_json::json;
use tokio::sync::oneshot;

use super::{Pending, route_response, wait_reply};
use crate::CodexError;

#[tokio::test(start_paused = true)]
async fn pending_call_timeout_removes_waiter() {
    let pending = Pending::default();
    let (sender, receiver) = oneshot::channel();
    pending.lock().await.insert(1, sender);
    let result = wait_reply(&pending, 1, "fixture/wait", receiver).await;
    assert!(matches!(result, Err(CodexError::Timeout(_))));
    assert!(pending.lock().await.is_empty());
}

#[tokio::test]
async fn pending_call_response_removes_waiter() -> anyhow::Result<()> {
    let pending = Pending::default();
    let (sender, receiver) = oneshot::channel();
    pending.lock().await.insert(1, sender);
    assert_eq!(pending.lock().await.len(), 1);
    route_response(&pending, 1, json!({"result": {}})).await;
    let result = wait_reply(&pending, 1, "fixture/answer", receiver).await?;
    assert_eq!(result, json!({"result": {}}));
    assert!(pending.lock().await.is_empty());
    Ok(())
}
