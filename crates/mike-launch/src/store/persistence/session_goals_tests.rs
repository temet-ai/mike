use crate::store::persistence::launch_repo::LaunchPersist as _;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::SetSessionIdentity;
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::TestDb;

fn fed(handle: &PersistHandle) -> ChangeFeed {
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

fn identity<'a>(
    session_uuid: Uuid,
    goal: Option<&'a str>,
    role: Option<&'a str>,
) -> SetSessionIdentity<'a> {
    SetSessionIdentity {
        session_uuid,
        display_name: None,
        role,
        goal,
        next_steps: None,
        instructions: None,
        set_by: "test",
    }
}

async fn goal_and_role(
    handle: &PersistHandle,
    session: Uuid,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let goal = handle.launch().get_session_goal(session).await?;
    Ok((
        goal.as_ref().map(|g| g.goal.clone()),
        goal.and_then(|g| g.role),
    ))
}

async fn two_handles(db_url: &str, session: Uuid) -> anyhow::Result<(PersistHandle, PersistHandle)> {
    let here = PersistHandle::connect(db_url).await?;
    let other = PersistHandle::connect(db_url).await?;
    here.launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    Ok((here, other))
}

fn owned(goal: &str, role: Option<&str>) -> (Option<String>, Option<String>) {
    (Some(goal.to_owned()), role.map(str::to_owned))
}

#[tokio::test]
async fn get_session_goal_second_read_without_notice_does_not_query() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let session = Uuid::new_v4();
    let (here, other) = two_handles(&db.database_url, session).await?;
    let _feed = fed(&here);
    other
        .launch()
        .set_session_goal(identity(session, Some("first goal"), Some("observer")))
        .await?;
    assert_eq!(goal_and_role(&here, session).await?, owned("first goal", Some("observer")));

    other
        .launch()
        .set_session_goal(identity(session, Some("second goal"), Some("worker")))
        .await?;
    assert_eq!(
        goal_and_role(&here, session).await?,
        owned("first goal", Some("observer")),
        "the change has not reached this process's feed, so the kept answer stands"
    );
    db.cleanup().await
}

#[tokio::test]
async fn get_session_goal_goal_and_role_change_after_notice_reads_new_values() -> anyhow::Result<()>
{
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let session = Uuid::new_v4();
    let bystander = Uuid::new_v4();
    let (here, other) = two_handles(&db.database_url, session).await?;
    let feed = fed(&here);
    assert_eq!(goal_and_role(&here, session).await?, (None, None));

    other
        .launch()
        .set_session_goal(identity(session, Some("ship it"), None))
        .await?;
    feed.record(RowChangedTable::SessionGoals, Some(&bystander.to_string()));
    feed.record(RowChangedTable::Sessions, Some(&session.to_string()));
    assert_eq!(
        goal_and_role(&here, session).await?,
        (None, None),
        "another session's goal notice and this session's sessions notice keep the answer"
    );

    feed.record(RowChangedTable::SessionGoals, Some(&session.to_string()));
    assert_eq!(goal_and_role(&here, session).await?, owned("ship it", None));

    other
        .launch()
        .set_session_goal(identity(session, None, Some("reviewer")))
        .await?;
    feed.record(RowChangedTable::SessionGoals, Some(&session.to_string()));
    assert_eq!(goal_and_role(&here, session).await?, owned("ship it", Some("reviewer")));
    db.cleanup().await
}

#[tokio::test]
async fn get_session_goal_after_own_write_reads_new_goal_at_once() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let session = Uuid::new_v4();
    let (here, _other) = two_handles(&db.database_url, session).await?;
    let _feed = fed(&here);
    here.launch()
        .set_session_goal(identity(session, Some("before"), None))
        .await?;
    assert_eq!(goal_and_role(&here, session).await?, owned("before", None));

    here.launch()
        .set_session_goal(identity(session, Some("after"), Some("lead")))
        .await?;
    assert_eq!(goal_and_role(&here, session).await?, owned("after", Some("lead")));
    db.cleanup().await
}

#[tokio::test]
async fn get_session_goal_without_feed_reads_every_time() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let session = Uuid::new_v4();
    let (here, other) = two_handles(&db.database_url, session).await?;
    other
        .launch()
        .set_session_goal(identity(session, Some("one"), None))
        .await?;
    assert_eq!(goal_and_role(&here, session).await?, owned("one", None));

    other
        .launch()
        .set_session_goal(identity(session, Some("two"), None))
        .await?;
    assert_eq!(goal_and_role(&here, session).await?, owned("two", None));
    db.cleanup().await
}
