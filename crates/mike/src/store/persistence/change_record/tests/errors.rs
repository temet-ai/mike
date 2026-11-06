use chrono::{Duration, Utc};

use super::fixture;
use crate::change_record::{ChangeKind, ChangeQuery};

#[tokio::test]
async fn every_function_reports_a_database_error_when_the_pool_is_closed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle.pool().close().await;
    let now = Utc::now();
    let query = ChangeQuery::everything_after("/work/alpha", 0, 5);
    assert!(
        f.handle
            .insert_change_events(&[super::seed::event(1, ChangeKind::Edit, &["a"])])
            .await
            .is_err()
    );
    assert!(f.handle.change_events_after(&query).await.is_err());
    assert!(f.handle.change_place(1, "/work/alpha").await.is_err());
    assert!(
        f.handle
            .move_change_place(1, "/work/alpha", 2)
            .await
            .is_err()
    );
    assert!(f.handle.paths_seen_by(1, now).await.is_err());
    assert!(
        f.handle
            .tool_calls_naming_path("/a", now, now)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .shell_commands_running("/work/alpha", now, now)
            .await
            .is_err()
    );
    assert!(f.handle.open_changes("/work/alpha").await.is_err());
    assert!(
        f.handle
            .checkouts_with_working_sessions(Duration::minutes(5))
            .await
            .is_err()
    );
    let detail = serde_json::json!({});
    assert!(
        f.handle
            .update_change_author(1, None, ChangeKind::Edit, &detail)
            .await
            .is_err()
    );
    f.db.cleanup().await?;
    Ok(())
}
