use crate::store::persistence::launch_repo::LaunchPersist as _;

use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use super::SetSessionIdentity;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::TestDb;

async fn next_goal_notice(listener: &mut PgListener, session: Uuid) -> anyhow::Result<String> {
    loop {
        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "session_goals"
            && payload["session_id"] == session.to_string().as_str()
        {
            return Ok(payload["op"].as_str().unwrap_or_default().to_owned());
        }
    }
}

fn goal(session_uuid: Uuid, goal: &str) -> SetSessionIdentity<'_> {
    SetSessionIdentity {
        session_uuid,
        display_name: None,
        role: None,
        goal: Some(goal),
        next_steps: None,
        instructions: None,
        set_by: "test",
    }
}

async fn session_with_listener(
    database_url: &str,
) -> anyhow::Result<(PersistHandle, PgListener, Uuid)> {
    let handle = PersistHandle::connect(database_url).await?;
    let session = Uuid::new_v4();
    handle
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen("proxy.row_changed").await?;
    Ok((handle, listener, session))
}

#[tokio::test]
async fn session_goals_trigger_insert_and_change_notify_the_session() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let (handle, mut listener, session) = session_with_listener(&db.database_url).await?;

    handle.launch().set_session_goal(goal(session, "first")).await?;
    assert_eq!(next_goal_notice(&mut listener, session).await?, "INSERT");

    handle.launch().set_session_goal(goal(session, "second")).await?;
    assert_eq!(next_goal_notice(&mut listener, session).await?, "UPDATE");
    db.cleanup().await
}

#[tokio::test]
async fn session_goals_trigger_rewrite_of_same_values_sends_nothing() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let (handle, mut listener, session) = session_with_listener(&db.database_url).await?;
    handle.launch().set_session_goal(goal(session, "same")).await?;
    assert_eq!(next_goal_notice(&mut listener, session).await?, "INSERT");

    handle.launch().set_session_goal(goal(session, "same")).await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query(
        "UPDATE sessions.session_goals sg SET set_by = 'elsewhere', updated_at = now()
           FROM sessions.sessions s
          WHERE s.id = sg.session_id AND s.session_uuid = $1",
    )
    .bind(session)
    .execute(&pool)
    .await?;
    sqlx::query(
        "DELETE FROM sessions.session_goals sg
          USING sessions.sessions s
          WHERE s.id = sg.session_id AND s.session_uuid = $1",
    )
    .bind(session)
    .execute(&pool)
    .await?;
    assert_eq!(
        next_goal_notice(&mut listener, session).await?,
        "DELETE",
        "the rewrites of unchanged values sent no notice before the delete"
    );
    db.cleanup().await
}

#[tokio::test]
async fn session_goals_trigger_session_delete_notifies_the_session() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let (handle, mut listener, session) = session_with_listener(&db.database_url).await?;
    handle.launch().set_session_goal(goal(session, "doomed")).await?;
    assert_eq!(next_goal_notice(&mut listener, session).await?, "INSERT");

    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query("DELETE FROM sessions.sessions WHERE session_uuid = $1")
        .bind(session)
        .execute(&pool)
        .await?;
    assert_eq!(next_goal_notice(&mut listener, session).await?, "DELETE");
    db.cleanup().await
}
