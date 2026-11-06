use std::time::Duration;

use crate::store::persistence::judge_ownership::JudgeOwnership;

pub(super) async fn lost(ownership: &mut JudgeOwnership) {
    loop {
        if let Err(error) = ownership.check_connection().await {
            tracing::warn!(%error, "judge worker: database ownership lost; stopping work");
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
