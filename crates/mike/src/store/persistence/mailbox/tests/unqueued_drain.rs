use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewLaunch;

use super::support::{drain_primary_turn, seed_session};

async fn drain_once(state: &crate::config::AppState, session: &str) -> Vec<String> {
    drain_primary_turn(state, session)
        .await
        .into_iter()
        .map(|r| r.payload)
        .collect()
}

async fn claimed(pool: &sqlx::PgPool, message: uuid::Uuid) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar!(
        r#"SELECT claimed_at IS NOT NULL AS "claimed!" FROM mailbox.messages WHERE message_uuid = $1"#,
        message,
    )
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn mail_written_only_to_the_database_is_injected_on_the_next_primary_turn_but_a_launch_brief_is_not()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let to = seed_session(&pool, "proj-drain-reads-the-record").await?;
    let (mail, brief) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    handle
        .insert_message(mail, to, None, "written by another mike process")
        .await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/drain-reads-the-record");
    launch.session_uuid = Some(to);
    launch.brief_note_uuid = Some(brief);
    handle.insert_launch(&launch).await?;
    handle
        .insert_message(brief, to, None, "the launch brief")
        .await?;

    let mut state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle.clone());
    let session = to.to_string();

    let injected = drain_once(&state, &session).await;
    assert_eq!(injected.len(), 1, "{injected:?}");
    assert!(injected[0].contains("written by another mike process"));
    assert!(claimed(&pool, mail).await?);
    assert!(!claimed(&pool, brief).await?);
    assert!(drain_once(&state, &session).await.is_empty());

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
