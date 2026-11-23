use uuid::Uuid;

use super::super::roster_test_support::{fed, insert_channel, live_names, register, unique_project};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::workspace::RosterScope;

#[tokio::test]
async fn roster_members_without_a_notice_are_not_read_again() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let session = Uuid::new_v4();
    let project = unique_project("members-held");
    let project_id = register(&here, session, &project).await?;
    let cwd = |rows: Vec<crate::store::persistence::workspace::WorkspaceRosterRow>| {
        rows.into_iter().map(|row| row.cwd).collect::<Vec<_>>()
    };
    assert_eq!(
        cwd(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![project.clone()]
    );

    sqlx::query(
        "UPDATE workspace.session_registrations SET cwd = cwd || '/moved'
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session)
    .execute(&here.pool)
    .await?;
    assert_eq!(
        cwd(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![project.clone()],
        "the move has not reached the feed"
    );

    feed.record(
        RowChangedTable::WorkspaceSessionRegistrations,
        Some(&session.to_string()),
    );
    assert_eq!(
        cwd(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![format!("{project}/moved")]
    );
    db.cleanup().await
}

#[tokio::test]
async fn roster_members_after_a_release_notice_leave_the_live_roster() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let project = unique_project("members-release");
    let kept = Uuid::new_v4();
    let leaving = Uuid::new_v4();
    let project_id = register(&here, kept, &project).await?;
    register(&here, leaving, &project).await?;
    let mut both = vec![kept, leaving];
    both.sort_unstable();
    assert_eq!(live_names(&here, project_id, RosterScope::Live).await?, both);

    other.end_workspace_registration(leaving, None).await?;
    assert_eq!(
        live_names(&here, project_id, RosterScope::Live).await?,
        both,
        "the release has not reached the feed"
    );

    feed.record(
        RowChangedTable::WorkspaceSessionRegistrations,
        Some(&leaving.to_string()),
    );
    assert_eq!(
        live_names(&here, project_id, RosterScope::Live).await?,
        vec![kept]
    );
    db.cleanup().await
}

#[tokio::test]
async fn roster_members_after_a_channel_link_notice_show_the_channel() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let session = Uuid::new_v4();
    let project_id = register(&here, session, &unique_project("members-link")).await?;
    let cse_id = format!("cse_link_{session}");
    insert_channel(&here, &cse_id, None, 0).await?;
    let channel_of = |rows: Vec<crate::store::persistence::workspace::WorkspaceRosterRow>| {
        rows.into_iter()
            .map(|row| row.channel_cse_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        channel_of(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![None]
    );

    sqlx::query("UPDATE remote_control.channels SET local_session_uuid = $2 WHERE cse_id = $1")
        .bind(&cse_id)
        .bind(session)
        .execute(&here.pool)
        .await?;
    assert_eq!(
        channel_of(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![None],
        "the link has not reached the feed"
    );

    feed.record(
        RowChangedTable::RemoteControlChannels,
        Some(&session.to_string()),
    );
    assert_eq!(
        channel_of(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec![Some(cse_id)]
    );
    db.cleanup().await
}

#[tokio::test]
async fn roster_heartbeats_without_a_notice_are_read_on_every_load() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let _feed = fed(&here);
    let session = Uuid::new_v4();
    let project_id = register(&here, session, &unique_project("members-beat")).await?;
    let cse_id = format!("cse_beat_{session}");
    insert_channel(&here, &cse_id, Some(session), 0).await?;
    let liveness = |rows: Vec<crate::store::persistence::workspace::WorkspaceRosterRow>| {
        rows.into_iter()
            .map(|row| row.liveness.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        liveness(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec!["live"]
    );

    sqlx::query(
        "UPDATE remote_control.channels SET last_heartbeat_at = now() - interval '5 minutes'
          WHERE cse_id = $1",
    )
    .bind(&cse_id)
    .execute(&here.pool)
    .await?;
    assert_eq!(
        liveness(here.fetch_workspace_roster(project_id, RosterScope::Live).await?),
        vec!["stale"]
    );
    db.cleanup().await
}
