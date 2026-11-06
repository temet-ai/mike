use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;

use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{boot_proxy, mailbox_row_state, ok_response};

#[tokio::test]
async fn open_create_covers_post_only_bare_and_notify_fanout() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping open_create_covers_post_only_bare_and_notify_fanout"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let project_id = seed_project(&pool, "open-threads-test").await?;
    let creator = uuid::Uuid::new_v4().to_string();
    let bob = uuid::Uuid::new_v4().to_string();
    let carol = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &creator).await?;
    seed_session(&pool, project_id, &bob).await?;
    seed_session(&pool, project_id, &carol).await?;

    assert_post_only_create(&client, &inspect_url, &pool, &creator).await?;
    assert_bare_create(&client, &inspect_url, &creator).await?;
    assert_notify_fanout(&client, &inspect_url, &creator, &bob, &carol).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn assert_post_only_create(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &PgPool,
    creator: &str,
) -> anyhow::Result<()> {
    let create_resp = client
        .post(format!("{inspect_url}/api/threads"))
        .json(&json!({
            "subject": "rollout plan",
            "body": "kickoff: let's plan the rollout",
            "from_session_id": creator,
            "tags": ["planning"],
            "notify": [],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        create_resp.status() == 201,
        "post-only create returned {}: {}",
        create_resp.status(),
        create_resp.text().await.unwrap_or_default()
    );
    let created: Value = create_resp.json().await?;
    let mut created_keys: Vec<&str> = created
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("response not an object: {created}"))?
        .keys()
        .map(String::as_str)
        .collect();
    created_keys.sort_unstable();
    anyhow::ensure!(
        created_keys == vec!["subject", "thread_uuid"],
        "POST /api/threads response must be the frozen {{thread_uuid, subject}} shape, \
         no deliveries/post_message_uuid leaking in: got {created_keys:?}"
    );
    let thread_uuid = created["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {created}"))?
        .to_string();
    anyhow::ensure!(created["subject"].as_str() == Some("rollout plan"));

    let detail_resp = client
        .get(format!("{inspect_url}/api/threads/{thread_uuid}"))
        .send()
        .await?;
    anyhow::ensure!(detail_resp.status() == 200);
    let detail: Value = detail_resp.json().await?;
    let messages = detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {detail}"))?;
    anyhow::ensure!(
        messages.len() == 1,
        "a post-only create has exactly its one recipient-less message: {detail}"
    );
    anyhow::ensure!(
        messages[0]["to_session_uuid"].is_null(),
        "the post-only row must have a null recipient: {detail}"
    );
    anyhow::ensure!(
        messages[0]["body"].as_str() == Some("kickoff: let's plan the rollout"),
        "body must round-trip verbatim: {detail}"
    );
    let thread_participants = detail["thread"]["participants"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participants not an array: {detail}"))?;
    anyhow::ensure!(
        thread_participants.len() == 1 && thread_participants[0].as_str() == Some(creator),
        "the only participant is the creator — a post-only message has no recipient to add: {detail}"
    );

    let (_, post_sent, post_dead_lettered) = {
        let msg_uuid = uuid::Uuid::parse_str(
            messages[0]["message_uuid"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {detail}"))?,
        )?;
        mailbox_row_state(pool, msg_uuid).await?
    };
    anyhow::ensure!(
        !post_sent && !post_dead_lettered,
        "a recipient-less post is never delivered or dead-lettered"
    );
    Ok(())
}

async fn assert_bare_create(
    client: &reqwest::Client,
    inspect_url: &str,
    creator: &str,
) -> anyhow::Result<()> {
    let bare_resp = client
        .post(format!("{inspect_url}/api/threads"))
        .json(&json!({
            "subject": "just opening the channel",
            "from_session_id": creator,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        bare_resp.status() == 201,
        "bare create returned {}",
        bare_resp.status()
    );
    let bare: Value = bare_resp.json().await?;
    let bare_thread_uuid = bare["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {bare}"))?
        .to_string();
    let bare_detail: Value = client
        .get(format!("{inspect_url}/api/threads/{bare_thread_uuid}"))
        .send()
        .await?
        .json()
        .await?;
    anyhow::ensure!(
        bare_detail["messages"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("messages not an array: {bare_detail}"))?
            .is_empty(),
        "a thread opened with no body and no notify has zero messages: {bare_detail}"
    );
    let bare_participants = bare_detail["thread"]["participants"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participants not an array: {bare_detail}"))?;
    anyhow::ensure!(
        bare_participants.len() == 1 && bare_participants[0].as_str() == Some(creator),
        "a bare thread's only participant is its creator, derived from created_by_session_id \
         with no message row to derive it from otherwise: {bare_detail}"
    );
    Ok(())
}

async fn assert_notify_fanout(
    client: &reqwest::Client,
    inspect_url: &str,
    creator: &str,
    bob: &str,
    carol: &str,
) -> anyhow::Result<()> {
    let notify_resp = client
        .post(format!("{inspect_url}/api/threads"))
        .json(&json!({
            "subject": "who's on call",
            "body": "paging both of you",
            "from_session_id": creator,
            "notify": [bob, carol],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        notify_resp.status() == 201,
        "notify create returned {}: {}",
        notify_resp.status(),
        notify_resp.text().await.unwrap_or_default()
    );
    let notify: Value = notify_resp.json().await?;
    let notify_thread_uuid = notify["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {notify}"))?
        .to_string();
    let notify_detail: Value = client
        .get(format!("{inspect_url}/api/threads/{notify_thread_uuid}"))
        .send()
        .await?
        .json()
        .await?;
    let notify_messages = notify_detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {notify_detail}"))?;
    anyhow::ensure!(
        notify_messages.len() == 2,
        "notify fan-out delivers one message per recipient, sharing the thread: {notify_detail}"
    );
    let mut recipients: Vec<Option<&str>> = notify_messages
        .iter()
        .map(|m| m["to_session_uuid"].as_str())
        .collect();
    recipients.sort_unstable();
    let mut expected = vec![Some(bob), Some(carol)];
    expected.sort_unstable();
    anyhow::ensure!(
        recipients == expected,
        "notify fan-out must deliver to exactly bob and carol: got {recipients:?}"
    );
    let notify_participants = notify_detail["thread"]["participants"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participants not an array: {notify_detail}"))?;
    anyhow::ensure!(
        notify_participants.len() == 3,
        "participants are the creator plus both notified recipients: {notify_detail}"
    );
    Ok(())
}

#[tokio::test]
async fn thread_contribution_to_empty_posts_without_delivery() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping thread_contribution_to_empty_posts_without_delivery"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let project_id = seed_project(&pool, "open-threads-contribution-test").await?;
    let alice = uuid::Uuid::new_v4().to_string();
    let bob = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &alice).await?;
    seed_session(&pool, project_id, &bob).await?;

    let (thread_uuid, post_message_uuid) =
        create_thread_and_post_only(&client, &inspect_url, &alice, &bob).await?;
    assert_default_fanout_and_post_only_state(
        &client,
        &inspect_url,
        &pool,
        &thread_uuid,
        &bob,
        &post_message_uuid,
    )
    .await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn create_thread_and_post_only(
    client: &reqwest::Client,
    inspect_url: &str,
    alice: &str,
    bob: &str,
) -> anyhow::Result<(String, String)> {
    let create_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "kickoff",
            "from_session_id": alice,
            "subject": "status thread",
        }))
        .send()
        .await?;
    anyhow::ensure!(create_resp.status() == 201);
    let threads: Value = client
        .get(format!("{inspect_url}/api/sessions/{alice}/threads"))
        .send()
        .await?
        .json()
        .await?;
    let thread_uuid = threads["threads"][0]["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {threads}"))?
        .to_string();

    let post_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/message"))
        .json(&json!({
            "body": "status update, for the record — not addressed to anyone",
            "from_session_id": alice,
            "to": [],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        post_resp.status() == 201,
        "explicit to:[] contribution returned {}: {}",
        post_resp.status(),
        post_resp.text().await.unwrap_or_default()
    );
    let post_body: Value = post_resp.json().await?;
    anyhow::ensure!(
        post_body["deliveries"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("deliveries not an array: {post_body}"))?
            .is_empty(),
        "an explicit to:[] post has no deliveries: {post_body}"
    );
    let post_message_uuid = post_body["post_message_uuid"]
        .as_str()
        .ok_or_else(|| {
            anyhow::anyhow!("post_message_uuid must be set on the post-only branch: {post_body}")
        })?
        .to_string();

    Ok((thread_uuid, post_message_uuid))
}

async fn assert_default_fanout_and_post_only_state(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &PgPool,
    thread_uuid: &str,
    bob: &str,
    post_message_uuid: &str,
) -> anyhow::Result<()> {
    let empty_default_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/message"))
        .json(&json!({
            "body": "should fail — no recipients and no explicit to",
            "from_session_id": bob,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        empty_default_resp.status() == 201,
        "default fan-out (to omitted) with an existing participant must still work: {}",
        empty_default_resp.status()
    );

    let detail: Value = client
        .get(format!("{inspect_url}/api/threads/{thread_uuid}"))
        .send()
        .await?
        .json()
        .await?;
    let messages = detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {detail}"))?;
    let post_row = messages
        .iter()
        .find(|m| m["message_uuid"].as_str() == Some(post_message_uuid))
        .ok_or_else(|| anyhow::anyhow!("post-only row missing from transcript: {detail}"))?;
    anyhow::ensure!(
        post_row["to_session_uuid"].is_null(),
        "the to:[] post must land with a null recipient in the transcript: {detail}"
    );

    let (post_claimed, post_sent, post_dead_lettered) =
        mailbox_row_state(pool, uuid::Uuid::parse_str(post_message_uuid)?).await?;
    anyhow::ensure!(
        !post_claimed && !post_sent && !post_dead_lettered,
        "a to:[] post is never claimed, delivered, or dead-lettered"
    );
    Ok(())
}
