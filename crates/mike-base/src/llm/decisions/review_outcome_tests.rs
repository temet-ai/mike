use serde_json::json;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};
use super::record::ReviewWrite;

#[tokio::test]
async fn review_outcome_writes_once_and_refuses_unknown_records() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let record = env.record(NewRecord::new("prod_detect", "sent")).await?;
    let store = super::store(&env.handle);
    let evidence = json!({"reason": "nudge matched the message", "reviewer": "s"});
    let first = store
        .insert_review_outcome(record, "right", &evidence)
        .await?;
    assert!(matches!(first, ReviewWrite::Inserted(_)));
    assert_eq!(
        store
            .insert_review_outcome(record, "wrong", &evidence)
            .await?,
        ReviewWrite::AlreadyReviewed
    );
    assert_eq!(
        store
            .insert_review_outcome(record, "right", &evidence)
            .await?,
        ReviewWrite::AlreadyReviewed
    );
    assert_eq!(
        store
            .insert_review_outcome(i64::MAX, "right", &evidence)
            .await?,
        ReviewWrite::UnknownRecord
    );
    assert_eq!(
        env.labels(record).await?,
        vec![pair("right", "claude_review")]
    );
    env.finish().await
}
