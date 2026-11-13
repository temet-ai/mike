use mike::store::persistence::proxy_repo::ProxyPersist as _;
use serde_json::json;
use uuid::Uuid;

use mike::channel_link_sweep::link::{ChannelLinkProgress, link_session_channels};
use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

fn attribution(id: &str) -> serde_json::Value {
    json!({"role":"user","content":[{"type":"text","text": format!(
        "<system-reminder>\nAttribution for git commits and pull requests you create from here on:\n- End git commit messages with:\nClaude-Session: https://claude.ai/code/session_{id}\n- End pull request descriptions with:\nhttps://claude.ai/code/session_{id}\n</system-reminder>"
    )}]})
}

fn tool_output(text: &str) -> serde_json::Value {
    json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":text}]})
}

fn body(messages: Vec<serde_json::Value>) -> Vec<u8> {
    json!({"messages": messages}).to_string().into_bytes()
}

fn suffix() -> String {
    Uuid::new_v4().simple().to_string()
}

async fn seed_channel(
    handle: &PersistHandle,
    cse_id: &str,
    session: Option<Uuid>,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1, $2)")
        .bind(cse_id)
        .bind(session)
        .execute(handle.pool())
        .await?;
    Ok(())
}

async fn owner_of(handle: &PersistHandle, cse_id: &str) -> anyhow::Result<Option<Uuid>> {
    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT local_session_uuid FROM remote_control.channels WHERE cse_id = $1",
    )
    .bind(cse_id)
    .fetch_one(handle.pool())
    .await?;
    Ok(owner)
}

#[tokio::test]
async fn resumed_session_links_the_new_channel_and_the_push_lookup_finds_it() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session = Uuid::new_v4();
    let (old_id, new_id) = (format!("OLD{}", suffix()), format!("NEW{}", suffix()));
    let (old_cse, new_cse) = (format!("cse_{old_id}"), format!("cse_{new_id}"));
    seed_channel(&handle, &old_cse, Some(session)).await?;
    seed_channel(&handle, &new_cse, None).await?;

    let progress = ChannelLinkProgress::new();
    let resumed = body(vec![
        attribution(&old_id),
        json!({"role":"assistant","content":[{"type":"text","text":"ok"}]}),
        attribution(&new_id),
    ]);
    let linked = link_session_channels(&handle, &progress, session, &resumed).await;
    assert_eq!(linked, 2);

    let snapshots = handle.proxy().fetch_channel_snapshots(&[session]).await?;
    let found: Vec<&str> = snapshots.iter().map(|s| s.cse_id.as_str()).collect();
    assert!(
        found.contains(&new_cse.as_str()),
        "new channel not found in {found:?}"
    );
    assert!(
        found.contains(&old_cse.as_str()),
        "old channel lost from {found:?}"
    );

    let again = link_session_channels(&handle, &progress, session, &resumed).await;
    assert_eq!(
        again, 0,
        "a body already scanned must cost no further writes"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_session_second_resume_links_the_third_channel() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session = Uuid::new_v4();
    let (first, second) = (format!("ONE{}", suffix()), format!("TWO{}", suffix()));
    let third = format!("THREE{}", suffix());
    for id in [&first, &second, &third] {
        seed_channel(&handle, &format!("cse_{id}"), None).await?;
    }

    let progress = ChannelLinkProgress::new();
    let after_first = body(vec![attribution(&first), attribution(&second)]);
    link_session_channels(&handle, &progress, session, &after_first).await;
    let after_second = body(vec![
        attribution(&first),
        attribution(&second),
        attribution(&third),
    ]);
    let linked = link_session_channels(&handle, &progress, session, &after_second).await;

    assert_eq!(linked, 1);
    assert_eq!(
        owner_of(&handle, &format!("cse_{third}")).await?,
        Some(session)
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_session_does_not_link_a_channel_named_only_in_a_tool_result() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session = Uuid::new_v4();
    let (own_id, foreign_id) = (format!("OWN{}", suffix()), format!("FOREIGN{}", suffix()));
    seed_channel(&handle, &format!("cse_{own_id}"), None).await?;
    seed_channel(&handle, &format!("cse_{foreign_id}"), None).await?;

    let progress = ChannelLinkProgress::new();
    let git_log = format!(
        "commit abc\n\n    fix\n\n    Claude-Session: https://claude.ai/code/session_{foreign_id}\n"
    );
    let b = body(vec![attribution(&own_id), tool_output(&git_log)]);
    link_session_channels(&handle, &progress, session, &b).await;

    assert_eq!(
        owner_of(&handle, &format!("cse_{own_id}")).await?,
        Some(session)
    );
    assert_eq!(owner_of(&handle, &format!("cse_{foreign_id}")).await?, None);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_session_leaves_a_channel_linked_to_another_session_alone() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let (session, other) = (Uuid::new_v4(), Uuid::new_v4());
    let taken_id = format!("TAKEN{}", suffix());
    let taken_cse = format!("cse_{taken_id}");
    seed_channel(&handle, &taken_cse, Some(other)).await?;

    let progress = ChannelLinkProgress::new();
    let b = body(vec![attribution(&taken_id)]);
    link_session_channels(&handle, &progress, session, &b).await;

    assert_eq!(owner_of(&handle, &taken_cse).await?, Some(other));
    assert!(
        handle
            .proxy()
            .fetch_channel_snapshots(&[session])
            .await?
            .is_empty()
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_session_links_a_reminder_appended_to_a_tool_result_but_not_its_git_trailers()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session = Uuid::new_v4();
    let (own_id, trailer_id) = (format!("OWN{}", suffix()), format!("TRAILER{}", suffix()));
    seed_channel(&handle, &format!("cse_{own_id}"), None).await?;
    seed_channel(&handle, &format!("cse_{trailer_id}"), None).await?;

    let reminder_text = attribution(&own_id)["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let output = format!(
        "commit abc\n\n    Claude-Session: https://claude.ai/code/session_{trailer_id}\n\n{reminder_text}"
    );
    let progress = ChannelLinkProgress::new();
    link_session_channels(
        &handle,
        &progress,
        session,
        &body(vec![tool_output(&output)]),
    )
    .await;

    assert_eq!(
        owner_of(&handle, &format!("cse_{own_id}")).await?,
        Some(session)
    );
    assert_eq!(owner_of(&handle, &format!("cse_{trailer_id}")).await?, None);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_session_does_not_link_another_sessions_reminder_printed_mid_output()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session = Uuid::new_v4();
    let foreign_id = format!("FOREIGN{}", suffix());
    seed_channel(&handle, &format!("cse_{foreign_id}"), None).await?;

    let printed = attribution(&foreign_id)["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let output = format!("{{\"text\": \"{printed}\"}}\nmore output after the dump");
    let progress = ChannelLinkProgress::new();
    let linked = link_session_channels(
        &handle,
        &progress,
        session,
        &body(vec![tool_output(&output)]),
    )
    .await;

    assert_eq!(linked, 0);
    assert_eq!(owner_of(&handle, &format!("cse_{foreign_id}")).await?, None);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
