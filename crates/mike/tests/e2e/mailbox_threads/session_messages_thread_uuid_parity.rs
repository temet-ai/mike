use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::ensure_session;

struct ThreadUuidParityFixture {
    dana: String,
    erin: String,
    threaded_uuid: String,
    reply_uuid: String,
    unthreaded_uuid: String,
    thread_uuid: String,
}

#[tokio::test]
async fn session_messages_thread_uuid_matches_thread_route_and_null_when_unthreaded()
-> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_threads");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let dana = "dd000000-0000-4000-d000-000000280010";
    let erin = "dd000000-0000-4000-d000-000000280011";
    ensure_session(&pool, dana, "mailbox-threads-test").await?;
    ensure_session(&pool, erin, "mailbox-threads-test").await?;

    let client = reqwest::Client::new();

    let fixture = seed_thread_uuid_parity_messages(&client, &inspect_url, dana, erin).await?;
    let threaded_row = assert_erin_inbox_thread_uuid(&client, &inspect_url, &fixture).await?;
    assert_dana_inbox_reply_uuid(&client, &inspect_url, &fixture).await?;
    assert_thread_route_matches_session_route(&client, &inspect_url, &fixture, &threaded_row)
        .await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn seed_thread_uuid_parity_messages(
    client: &reqwest::Client,
    inspect_url: &str,
    dana: &str,
    erin: &str,
) -> anyhow::Result<ThreadUuidParityFixture> {
    let threaded_resp = client
        .post(format!("{inspect_url}/api/sessions/{erin}/message"))
        .json(&json!({
            "message": "kickoff for the thread-uuid regression",
            "from_session_id": dana,
            "subject": "thread-uuid regression",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        threaded_resp.status() == 201,
        "threaded send returned {}",
        threaded_resp.status()
    );
    let threaded: Value = threaded_resp.json().await?;
    let threaded_uuid = threaded["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {threaded}"))?
        .to_string();

    let reply_resp = client
        .post(format!("{inspect_url}/api/sessions/{dana}/message"))
        .json(&json!({
            "message": "replying in-thread",
            "from_session_id": erin,
            "in_reply_to": threaded_uuid,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        reply_resp.status() == 201,
        "reply send returned {}",
        reply_resp.status()
    );
    let reply: Value = reply_resp.json().await?;
    let reply_uuid = reply["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {reply}"))?
        .to_string();

    let unthreaded_resp = client
        .post(format!("{inspect_url}/api/sessions/{erin}/message"))
        .json(&json!({ "message": "plain unthreaded message", "from_session_id": dana }))
        .send()
        .await?;
    anyhow::ensure!(
        unthreaded_resp.status() == 201,
        "unthreaded send returned {}",
        unthreaded_resp.status()
    );
    let unthreaded: Value = unthreaded_resp.json().await?;
    let unthreaded_uuid = unthreaded["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {unthreaded}"))?
        .to_string();

    let threads_resp = client
        .get(format!("{inspect_url}/api/sessions/{erin}/threads"))
        .send()
        .await?;
    anyhow::ensure!(
        threads_resp.status() == 200,
        "get_session_threads returned {}",
        threads_resp.status()
    );
    let threads: Value = threads_resp.json().await?;
    let thread_uuid = threads["threads"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("threads not an array: {threads}"))?
        .iter()
        .find(|t| t["subject"].as_str() == Some("thread-uuid regression"))
        .and_then(|t| t["thread_uuid"].as_str())
        .ok_or_else(|| anyhow::anyhow!("thread not found: {threads}"))?
        .to_string();

    Ok(ThreadUuidParityFixture {
        dana: dana.to_string(),
        erin: erin.to_string(),
        threaded_uuid,
        reply_uuid,
        unthreaded_uuid,
        thread_uuid,
    })
}

async fn assert_erin_inbox_thread_uuid(
    client: &reqwest::Client,
    inspect_url: &str,
    fixture: &ThreadUuidParityFixture,
) -> anyhow::Result<Value> {
    let erin_messages_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{}/messages",
            fixture.erin
        ))
        .send()
        .await?;
    anyhow::ensure!(
        erin_messages_resp.status() == 200,
        "get_session_messages (erin) returned {}",
        erin_messages_resp.status()
    );
    let erin_body: Value = erin_messages_resp.json().await?;
    let erin_inbox = erin_body["inbox"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox not an array: {erin_body}"))?;

    let threaded_row = erin_inbox
        .iter()
        .find(|r| r["message_uuid"].as_str() == Some(fixture.threaded_uuid.as_str()))
        .ok_or_else(|| anyhow::anyhow!("threaded message missing from inbox: {erin_body}"))?;
    anyhow::ensure!(
        threaded_row["thread_uuid"].as_str() == Some(fixture.thread_uuid.as_str()),
        "inbox row's thread_uuid must match GET /api/threads/{{uuid}} for the same message: {threaded_row}"
    );

    let unthreaded_row = erin_inbox
        .iter()
        .find(|r| r["message_uuid"].as_str() == Some(fixture.unthreaded_uuid.as_str()))
        .ok_or_else(|| anyhow::anyhow!("unthreaded message missing from inbox: {erin_body}"))?;
    anyhow::ensure!(
        unthreaded_row["thread_uuid"].is_null(),
        "a genuinely unthreaded message must NOT be fabricated a thread_uuid: {unthreaded_row}"
    );

    Ok(threaded_row.clone())
}

async fn assert_dana_inbox_reply_uuid(
    client: &reqwest::Client,
    inspect_url: &str,
    fixture: &ThreadUuidParityFixture,
) -> anyhow::Result<()> {
    let dana_messages_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{}/messages",
            fixture.dana
        ))
        .send()
        .await?;
    anyhow::ensure!(
        dana_messages_resp.status() == 200,
        "get_session_messages (dana) returned {}",
        dana_messages_resp.status()
    );
    let dana_body: Value = dana_messages_resp.json().await?;
    let dana_inbox = dana_body["inbox"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox not an array: {dana_body}"))?;
    let reply_row = dana_inbox
        .iter()
        .find(|r| r["message_uuid"].as_str() == Some(fixture.reply_uuid.as_str()))
        .ok_or_else(|| anyhow::anyhow!("reply missing from dana's inbox: {dana_body}"))?;
    anyhow::ensure!(
        reply_row["in_reply_to_uuid"].as_str() == Some(fixture.threaded_uuid.as_str()),
        "in_reply_to_uuid must point at the parent message: {reply_row}"
    );
    anyhow::ensure!(
        reply_row["thread_uuid"].as_str() == Some(fixture.thread_uuid.as_str()),
        "the reply's thread_uuid must match the thread it was posted into: {reply_row}"
    );
    Ok(())
}

async fn assert_thread_route_matches_session_route(
    client: &reqwest::Client,
    inspect_url: &str,
    fixture: &ThreadUuidParityFixture,
    threaded_row: &Value,
) -> anyhow::Result<()> {
    let thread_detail_resp = client
        .get(format!("{inspect_url}/api/threads/{}", fixture.thread_uuid))
        .send()
        .await?;
    anyhow::ensure!(
        thread_detail_resp.status() == 200,
        "get_thread returned {}",
        thread_detail_resp.status()
    );
    let thread_detail: Value = thread_detail_resp.json().await?;
    let transcript = thread_detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {thread_detail}"))?;
    let transcript_row = transcript
        .iter()
        .find(|r| r["message_uuid"].as_str() == Some(fixture.threaded_uuid.as_str()))
        .ok_or_else(|| {
            anyhow::anyhow!("threaded message missing from transcript: {thread_detail}")
        })?;
    anyhow::ensure!(
        transcript_row["thread_uuid"] == threaded_row["thread_uuid"],
        "GET /api/sessions/{{id}}/messages and GET /api/threads/{{uuid}} must agree on \
         thread_uuid for the same message_uuid: session-route={threaded_row}, \
         thread-route={transcript_row}"
    );
    Ok(())
}
