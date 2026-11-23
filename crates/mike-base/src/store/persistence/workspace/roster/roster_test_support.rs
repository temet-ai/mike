use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::RegisterWorkspaceSession;
use crate::workspace::RosterScope;

pub(super) fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        CancellationToken::new(),
        &TaskTracker::new(),
    );
    feed
}

pub(super) async fn register(
    handle: &PersistHandle,
    session_uuid: Uuid,
    project: &str,
) -> anyhow::Result<i64> {
    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid,
            project_path: Some(project),
            cwd: project,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(registered.project_id)
}

pub(super) async fn insert_channel(
    handle: &PersistHandle,
    cse_id: &str,
    session_uuid: Option<Uuid>,
    age_seconds: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO remote_control.channels
             (cse_id, local_session_uuid, first_seen_at, last_seen_at,
              last_heartbeat_at, last_heartbeat_interval_seconds, worker_status, worker_status_at)
         VALUES ($1, $2, now(), now(), now() - make_interval(secs => $3), 20, 'idle', now())",
    )
    .bind(cse_id)
    .bind(session_uuid)
    .bind(f64::from(age_seconds))
    .execute(&handle.pool)
    .await?;
    Ok(())
}

pub(super) async fn live_names(
    handle: &PersistHandle,
    project_id: i64,
    scope: RosterScope,
) -> anyhow::Result<Vec<Uuid>> {
    let mut names: Vec<Uuid> = handle
        .fetch_workspace_roster(project_id, scope)
        .await?
        .into_iter()
        .map(|row| row.session_uuid)
        .collect();
    names.sort_unstable();
    Ok(names)
}

pub(super) fn unique_project(label: &str) -> String {
    format!("/tmp/roster-{label}-{}", Uuid::new_v4())
}
