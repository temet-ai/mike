use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use super::roster_test_support::{insert_channel, register, unique_project};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn next_for(listener: &mut PgListener, session: Uuid) -> anyhow::Result<String> {
    loop {
        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["session_id"] == session.to_string().as_str() {
            return Ok(payload["table"].as_str().unwrap_or_default().to_owned());
        }
    }
}

async fn listening(url: &str) -> anyhow::Result<PgListener> {
    let mut listener = PgListener::connect(url).await?;
    listener.listen("proxy.row_changed").await?;
    Ok(listener)
}

async fn set_lease(handle: &PersistHandle, session: Uuid, minutes: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE workspace.session_registrations
            SET lease_expires_at = now() + make_interval(mins => $2)
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session)
    .bind(minutes)
    .execute(&handle.pool)
    .await?;
    Ok(())
}

async fn beat(handle: &PersistHandle, cse_id: &str, seconds_ago: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE remote_control.channels
            SET last_heartbeat_at = now() - make_interval(secs => $2)
          WHERE cse_id = $1",
    )
    .bind(cse_id)
    .bind(f64::from(seconds_ago))
    .execute(&handle.pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn registration_notices_skip_a_live_lease_renewal_and_name_a_release() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    register(&handle, session, &unique_project("notice-lease")).await?;
    let mut listener = listening(&db.database_url).await?;

    set_lease(&handle, session, 30).await?;
    insert_channel(&handle, &format!("cse_mark_{session}"), Some(session), 0).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "remote_control_channels",
        "a renewal of a live lease sends nothing"
    );

    set_lease(&handle, session, -1).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "workspace_session_registrations",
        "a shortened lease is a notice"
    );
    set_lease(&handle, session, 15).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "workspace_session_registrations",
        "a renewal of a lapsed lease is a notice"
    );
    handle.end_workspace_registration(session, None).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "workspace_session_registrations"
    );
    db.cleanup().await
}

#[tokio::test]
async fn channel_notices_name_a_link_and_a_revived_heartbeat_but_not_a_steady_one()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    register(&handle, session, &unique_project("notice-channel")).await?;
    let cse_id = format!("cse_notice_{session}");
    insert_channel(&handle, &cse_id, None, 0).await?;
    let mut listener = listening(&db.database_url).await?;

    sqlx::query("UPDATE remote_control.channels SET local_session_uuid = $2 WHERE cse_id = $1")
        .bind(&cse_id)
        .bind(session)
        .execute(&handle.pool)
        .await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "remote_control_channels"
    );

    beat(&handle, &cse_id, 0).await?;
    beat(&handle, &cse_id, 300).await?;
    handle.end_workspace_registration(session, None).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "workspace_session_registrations",
        "heartbeats inside the window send nothing"
    );

    beat(&handle, &cse_id, 0).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "remote_control_channels",
        "a heartbeat after silence is a notice"
    );
    db.cleanup().await
}

#[tokio::test]
async fn session_facts_notices_name_a_new_row_and_an_ended_lifecycle() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    register(&handle, session, &unique_project("notice-facts")).await?;
    let mut listener = listening(&db.database_url).await?;
    let facts = |sql: &'static str| {
        sqlx::query(sql)
            .bind(session)
            .execute(&handle.pool)
    };

    facts(
        "INSERT INTO runtime.session_facts
             (session_id, runtime, source, fidelity, completeness, observed_at, sequence, lifecycle)
         SELECT id, 'codex', 'app_server', 'native', 'complete', now(), 1, 'active'
           FROM sessions.sessions WHERE session_uuid = $1",
    )
    .await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "runtime_session_facts"
    );

    facts(
        "UPDATE runtime.session_facts SET lifecycle = 'idle', observed_at = now()
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .await?;
    facts(
        "UPDATE runtime.session_facts SET lifecycle = 'ended'
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "runtime_session_facts"
    );
    handle.end_workspace_registration(session, None).await?;
    assert_eq!(
        next_for(&mut listener, session).await?,
        "workspace_session_registrations",
        "an idle turn sends nothing, so the ended lifecycle sent the one notice"
    );
    db.cleanup().await
}
