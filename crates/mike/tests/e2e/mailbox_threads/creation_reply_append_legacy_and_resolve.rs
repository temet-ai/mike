use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

use super::support::ensure_session;

#[tokio::test]
async fn threads_cover_creation_reply_append_legacy_and_resolve() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_threads");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let alice = "dd000000-0000-4000-d000-000000280001";
    let bob = "dd000000-0000-4000-d000-000000280002";
    let carol = "dd000000-0000-4000-d000-000000280003";
    ensure_session(&pool, alice, "mailbox-threads-test").await?;
    ensure_session(&pool, bob, "mailbox-threads-test").await?;
    ensure_session(&pool, carol, "mailbox-threads-test").await?;

    let client = reqwest::Client::new();

    let (first_message_uuid, thread_uuid) =
        assert_subject_creates_thread(&client, &inspect_url, alice, bob).await?;
    assert_thread_id_appends_and_unknown_404(&client, &inspect_url, alice, bob, &thread_uuid)
        .await?;
    assert_in_reply_to_retroactive_thread(&client, &inspect_url, &pool, alice, carol).await?;
    assert_legacy_send_has_no_thread(&client, &inspect_url, &pool, bob).await?;
    let first_resolved_at =
        assert_resolve_is_idempotent(&client, &inspect_url, &thread_uuid).await?;
    assert_transcript_oldest_first(
        &client,
        &inspect_url,
        &thread_uuid,
        &first_message_uuid,
        &first_resolved_at,
    )
    .await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn assert_subject_creates_thread(
    client: &reqwest::Client,
    inspect_url: &str,
    alice: &str,
    bob: &str,
) -> anyhow::Result<(String, String)> {
    let create_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "kickoff: let's plan the rollout",
            "from_session_id": alice,
            "subject": "rollout plan",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        create_resp.status() == 201,
        "subject send returned {}",
        create_resp.status()
    );
    let created: Value = create_resp.json().await?;
    let mut created_keys: Vec<&str> = created
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("response not an object: {created}"))?
        .keys()
        .map(String::as_str)
        .collect();
    created_keys.sort_unstable();
    let mut expected_keys = vec![
        "message_uuid",
        "to_session_uuid",
        "from_session_uuid",
        "queued_at",
        "recipient_liveness",
        "recipient_last_request_at",
        "recipient_last_eligible_turn_at",
        "delivery_state",
    ];
    expected_keys.sort_unstable();
    anyhow::ensure!(
        created_keys == expected_keys,
        "POST /api/sessions/{{to}}/message response shape must stay byte-identical \
         regardless of threading — got keys {created_keys:?}, expected {expected_keys:?}"
    );

    let first_message_uuid = created["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {created}"))?
        .to_string();

    let threads_resp = client
        .get(format!("{inspect_url}/api/sessions/{alice}/threads"))
        .send()
        .await?;
    anyhow::ensure!(
        threads_resp.status() == 200,
        "get_session_threads returned {}",
        threads_resp.status()
    );
    let threads: Value = threads_resp.json().await?;
    let thread_list = threads["threads"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("threads field not an array: {threads}"))?;
    anyhow::ensure!(
        thread_list.len() == 1,
        "alice must participate in exactly one thread so far: {threads}"
    );
    let thread_entry = &thread_list[0];
    anyhow::ensure!(
        thread_entry["subject"].as_str() == Some("rollout plan"),
        "thread subject must be the one supplied at creation: {thread_entry}"
    );
    anyhow::ensure!(
        thread_entry["message_count"].as_i64() == Some(1),
        "a freshly created thread has exactly its first message: {thread_entry}"
    );
    anyhow::ensure!(
        thread_entry["resolved_at"].is_null(),
        "a freshly created thread is not resolved: {thread_entry}"
    );
    let thread_uuid = thread_entry["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {thread_entry}"))?
        .to_string();
    let participants = thread_entry["participants"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participants not an array: {thread_entry}"))?;
    anyhow::ensure!(
        participants.len() == 2
            && participants.iter().any(|p| p.as_str() == Some(alice))
            && participants.iter().any(|p| p.as_str() == Some(bob)),
        "participants must be derived from the thread's messages (alice + bob): {thread_entry}"
    );

    Ok((first_message_uuid, thread_uuid))
}

async fn assert_thread_id_appends_and_unknown_404(
    client: &reqwest::Client,
    inspect_url: &str,
    alice: &str,
    bob: &str,
    thread_uuid: &str,
) -> anyhow::Result<()> {
    let append_resp = client
        .post(format!("{inspect_url}/api/sessions/{alice}/message"))
        .json(&json!({
            "message": "bob, any objections?",
            "from_session_id": bob,
            "thread_id": thread_uuid,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        append_resp.status() == 201,
        "thread_id append returned {}",
        append_resp.status()
    );

    let unknown_thread = "eeeeeeee-0000-4000-8000-000000000000";
    let unknown_resp = client
        .post(format!("{inspect_url}/api/sessions/{alice}/message"))
        .json(&json!({
            "message": "should never land",
            "thread_id": unknown_thread,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        unknown_resp.status() == 404,
        "an unknown thread_id must 404, got {}",
        unknown_resp.status()
    );
    let unknown_body: Value = unknown_resp.json().await?;
    anyhow::ensure!(
        unknown_body["error"].as_str() == Some("thread_not_found"),
        "unknown thread_id error code must be thread_not_found: {unknown_body}"
    );
    Ok(())
}

async fn assert_in_reply_to_retroactive_thread(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &PgPool,
    alice: &str,
    carol: &str,
) -> anyhow::Result<()> {
    let plain_resp = client
        .post(format!("{inspect_url}/api/sessions/{carol}/message"))
        .json(&json!({
            "message": "unrelated heads-up, no thread yet",
            "from_session_id": alice,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        plain_resp.status() == 201,
        "plain send returned {}",
        plain_resp.status()
    );
    let plain: Value = plain_resp.json().await?;
    let plain_message_uuid = plain["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {plain}"))?
        .to_string();

    let reply_resp = client
        .post(format!("{inspect_url}/api/sessions/{alice}/message"))
        .json(&json!({
            "message": "replying to your heads-up",
            "from_session_id": carol,
            "in_reply_to": plain_message_uuid,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        reply_resp.status() == 201,
        "in_reply_to send returned {}",
        reply_resp.status()
    );

    let (parent_thread_id,): (Option<i64>,) = sqlx::query_as(
        "SELECT thread_id FROM mailbox.messages WHERE message_uuid = $1::text::uuid",
    )
    .bind(&plain_message_uuid)
    .fetch_one(pool)
    .await?;
    let parent_thread_id = parent_thread_id
        .ok_or_else(|| anyhow::anyhow!("parent must have been adopted into a thread"))?;

    let reply_body: Value = reply_resp.json().await?;
    let reply_message_uuid = reply_body["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid on reply response: {reply_body}"))?;
    let (reply_thread_id,): (Option<i64>,) = sqlx::query_as(
        "SELECT thread_id FROM mailbox.messages WHERE message_uuid = $1::text::uuid",
    )
    .bind(reply_message_uuid)
    .fetch_one(pool)
    .await?;
    let reply_thread_id = reply_thread_id
        .ok_or_else(|| anyhow::anyhow!("reply must carry the retroactively created thread_id"))?;
    anyhow::ensure!(
        parent_thread_id == reply_thread_id,
        "retroactive adoption must land the parent and the reply in the SAME thread: \
         parent={parent_thread_id} reply={reply_thread_id}"
    );

    let retro_subject: (String,) =
        sqlx::query_as("SELECT subject FROM mailbox.threads WHERE id = $1")
            .bind(parent_thread_id)
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(
        retro_subject.0 == "unrelated heads-up, no thread yet",
        "retroactive subject must be the parent body's first 80 chars verbatim: {:?}",
        retro_subject.0
    );
    Ok(())
}

async fn assert_legacy_send_has_no_thread(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &PgPool,
    bob: &str,
) -> anyhow::Result<()> {
    let legacy_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({ "message": "plain unthreaded message" }))
        .send()
        .await?;
    anyhow::ensure!(
        legacy_resp.status() == 201,
        "legacy send returned {}",
        legacy_resp.status()
    );
    let legacy: Value = legacy_resp.json().await?;
    let legacy_message_uuid = legacy["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {legacy}"))?
        .to_string();
    let legacy_thread_id: (Option<i64>,) = sqlx::query_as(
        "SELECT thread_id FROM mailbox.messages WHERE message_uuid = $1::text::uuid",
    )
    .bind(&legacy_message_uuid)
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        legacy_thread_id.0.is_none(),
        "a send with no threading param must leave thread_id NULL"
    );
    Ok(())
}

async fn assert_resolve_is_idempotent(
    client: &reqwest::Client,
    inspect_url: &str,
    thread_uuid: &str,
) -> anyhow::Result<String> {
    let resolve_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/resolve"))
        .send()
        .await?;
    anyhow::ensure!(
        resolve_resp.status() == 200,
        "resolve returned {}",
        resolve_resp.status()
    );
    let resolved: Value = resolve_resp.json().await?;
    let first_resolved_at = resolved["resolved_at"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing resolved_at: {resolved}"))?
        .to_string();

    let resolve_again_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/resolve"))
        .send()
        .await?;
    anyhow::ensure!(
        resolve_again_resp.status() == 200,
        "repeat resolve returned {}",
        resolve_again_resp.status()
    );
    let resolved_again: Value = resolve_again_resp.json().await?;
    anyhow::ensure!(
        resolved_again["resolved_at"].as_str() == Some(first_resolved_at.as_str()),
        "repeat resolve must return the ORIGINAL resolved_at, not re-stamp: first={first_resolved_at} second={resolved_again}"
    );
    Ok(first_resolved_at)
}

async fn assert_transcript_oldest_first(
    client: &reqwest::Client,
    inspect_url: &str,
    thread_uuid: &str,
    first_message_uuid: &str,
    first_resolved_at: &str,
) -> anyhow::Result<()> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/threads/{thread_uuid}"))
        .send()
        .await?;
    anyhow::ensure!(
        detail_resp.status() == 200,
        "get_thread returned {}",
        detail_resp.status()
    );
    let detail: Value = detail_resp.json().await?;
    anyhow::ensure!(
        detail["thread"]["resolved_at"].as_str() == Some(first_resolved_at),
        "thread detail must reflect the resolve above: {detail}"
    );
    let messages = detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {detail}"))?;
    anyhow::ensure!(
        messages.len() == 2,
        "the rollout-plan thread has exactly its two messages (kickoff + the thread_id append): {detail}"
    );
    anyhow::ensure!(
        messages[0]["message_uuid"].as_str() == Some(first_message_uuid),
        "messages must be oldest-first — the kickoff message comes first: {detail}"
    );
    anyhow::ensure!(
        messages[0]["thread_uuid"].as_str() == Some(thread_uuid),
        "each transcript row carries the thread_uuid: {detail}"
    );
    anyhow::ensure!(
        messages[1]["body"].as_str() == Some("bob, any objections?"),
        "the second message is the thread_id append, body verbatim: {detail}"
    );
    Ok(())
}
