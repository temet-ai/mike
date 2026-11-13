use crate::store::persistence::fleet_repo::FleetPersist as _;
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
            .fleet()
            .insert_change_events(&[super::seed::event(1, ChangeKind::Edit, &["a"])])
            .await
            .is_err()
    );
    assert!(f.handle.fleet().change_events_after(&query).await.is_err());
    assert!(
        f.handle
            .fleet()
            .change_place(1, "/work/alpha")
            .await
            .is_err()
    );
    assert!(
        f.handle
            .fleet()
            .move_change_place(1, "/work/alpha", 2)
            .await
            .is_err()
    );
    assert!(f.handle.fleet().paths_seen_by(1, now).await.is_err());
    assert!(
        f.handle
            .fleet()
            .tool_calls_naming_path("/a", now, now)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .fleet()
            .shell_commands_running("/work/alpha", now, now)
            .await
            .is_err()
    );
    assert!(f.handle.fleet().open_changes("/work/alpha").await.is_err());
    assert!(
        f.handle
            .fleet()
            .checkouts_with_working_sessions(Duration::minutes(5))
            .await
            .is_err()
    );
    let detail = serde_json::json!({});
    assert!(
        f.handle
            .fleet()
            .update_change_author(1, None, ChangeKind::Edit, &detail)
            .await
            .is_err()
    );
    f.db.cleanup().await?;
    Ok(())
}
