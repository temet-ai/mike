use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

use super::support::{context_items_for, ensure_session};

struct Ctx<'a> {
    client: &'a reqwest::Client,
    inspect_url: &'a str,
}

async fn assert_session_route_context_round_trips(
    ctx: &Ctx<'_>,
    bob: &str,
    alice: &str,
) -> anyhow::Result<()> {
    let context = json!([
        {"kind": "request", "value": "12345", "label": "the failing request"},
        {"kind": "url", "value": "https://example.com/runbook", "label": null},
    ]);
    let send_resp = ctx
        .client
        .post(format!("{}/api/sessions/{bob}/message", ctx.inspect_url))
        .json(&json!({
            "message": "see the failing request",
            "from_session_id": alice,
            "context": context,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        send_resp.status() == 201,
        "context-bearing send returned {}",
        send_resp.status()
    );
    let sent: Value = send_resp.json().await?;
    let sent_uuid = sent["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {sent}"))?
        .to_string();

    let inbox_resp = ctx
        .client
        .get(format!("{}/api/sessions/{bob}/messages", ctx.inspect_url))
        .send()
        .await?;
    anyhow::ensure!(
        inbox_resp.status() == 200,
        "get_session_messages returned {}",
        inbox_resp.status()
    );
    let inbox_body: Value = inbox_resp.json().await?;
    let inbox_rows = inbox_body["inbox"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox not an array: {inbox_body}"))?;
    let round_tripped = context_items_for(inbox_rows, &sent_uuid)
        .ok_or_else(|| anyhow::anyhow!("sent message missing from inbox: {inbox_body}"))?;
    anyhow::ensure!(
        round_tripped == &context,
        "context_items must round-trip verbatim on the session route: got {round_tripped}, want {context}"
    );
    Ok(())
}

async fn assert_bare_send_has_empty_context_items(ctx: &Ctx<'_>, bob: &str) -> anyhow::Result<()> {
    let bare_resp = ctx
        .client
        .post(format!("{}/api/sessions/{bob}/message", ctx.inspect_url))
        .json(&json!({ "message": "no context here" }))
        .send()
        .await?;
    anyhow::ensure!(
        bare_resp.status() == 201,
        "bare send returned {}",
        bare_resp.status()
    );
    let bare: Value = bare_resp.json().await?;
    let bare_uuid = bare["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {bare}"))?
        .to_string();
    let inbox_resp2 = ctx
        .client
        .get(format!("{}/api/sessions/{bob}/messages", ctx.inspect_url))
        .send()
        .await?;
    let inbox_body2: Value = inbox_resp2.json().await?;
    let inbox_rows2 = inbox_body2["inbox"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox not an array: {inbox_body2}"))?;
    let bare_context = context_items_for(inbox_rows2, &bare_uuid)
        .ok_or_else(|| anyhow::anyhow!("bare message missing from inbox: {inbox_body2}"))?;
    anyhow::ensure!(
        bare_context.as_array() == Some(&vec![]),
        "no-context send must still carry context_items: [], got {bare_context}"
    );
    Ok(())
}

async fn assert_session_route_rejects_bad_context_kind(
    ctx: &Ctx<'_>,
    bob: &str,
) -> anyhow::Result<()> {
    let bad_kind_resp = ctx
        .client
        .post(format!("{}/api/sessions/{bob}/message", ctx.inspect_url))
        .json(&json!({
            "message": "should never land",
            "context": [{"kind": "bogus", "value": "x"}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        bad_kind_resp.status() == 400,
        "unknown context kind must 400 on the session route, got {}",
        bad_kind_resp.status()
    );
    let bad_kind_body: Value = bad_kind_resp.json().await?;
    anyhow::ensure!(
        bad_kind_body["error"].as_str() == Some("invalid_context_kind"),
        "error code must be invalid_context_kind: {bad_kind_body}"
    );
    anyhow::ensure!(
        bad_kind_body["message"]
            .as_str()
            .is_some_and(|m| m.contains("bogus")),
        "error message must name the offending kind: {bad_kind_body}"
    );
    Ok(())
}

async fn create_thread_with_context_reply(
    ctx: &Ctx<'_>,
    pool: &PgPool,
    alice: &str,
    bob: &str,
) -> anyhow::Result<(String, String, String)> {
    let thread_create_resp = ctx
        .client
        .post(format!("{}/api/sessions/{bob}/message", ctx.inspect_url))
        .json(&json!({
            "message": "message A — thread root",
            "from_session_id": alice,
            "subject": "context + reply chain thread",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        thread_create_resp.status() == 201,
        "thread create returned {}",
        thread_create_resp.status()
    );
    let threads_resp = ctx
        .client
        .get(format!("{}/api/sessions/{alice}/threads", ctx.inspect_url))
        .send()
        .await?;
    let threads: Value = threads_resp.json().await?;
    let thread_uuid = threads["threads"][0]["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {threads}"))?
        .to_string();

    let (message_a_uuid,): (String,) = sqlx::query_as(
        "SELECT message_uuid::text FROM mailbox.messages
          WHERE thread_id = (SELECT id FROM mailbox.threads WHERE thread_uuid = $1::text::uuid)
          ORDER BY id ASC LIMIT 1",
    )
    .bind(&thread_uuid)
    .fetch_one(pool)
    .await?;

    let thread_context = json!([{"kind": "commit", "value": "abc123", "label": null}]);
    let thread_send_resp = ctx
        .client
        .post(format!(
            "{}/api/threads/{thread_uuid}/message",
            ctx.inspect_url
        ))
        .json(&json!({
            "body": "message B — replies to A, carries context",
            "from_session_id": bob,
            "in_reply_to": message_a_uuid,
            "context": thread_context,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        thread_send_resp.status() == 201,
        "thread-route context send returned {}",
        thread_send_resp.status()
    );
    let thread_sent: Value = thread_send_resp.json().await?;
    let message_b_uuid = thread_sent["deliveries"][0]["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing delivery message_uuid: {thread_sent}"))?
        .to_string();

    let detail_resp = ctx
        .client
        .get(format!("{}/api/threads/{thread_uuid}", ctx.inspect_url))
        .send()
        .await?;
    anyhow::ensure!(
        detail_resp.status() == 200,
        "get_thread returned {}",
        detail_resp.status()
    );
    let detail: Value = detail_resp.json().await?;
    let messages = detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {detail}"))?;
    let b_context = context_items_for(messages, &message_b_uuid)
        .ok_or_else(|| anyhow::anyhow!("message B missing from transcript: {detail}"))?;
    anyhow::ensure!(
        b_context == &thread_context,
        "context_items must round-trip verbatim on the thread route: got {b_context}, want {thread_context}"
    );
    let b_row = messages
        .iter()
        .find(|r| r["message_uuid"].as_str() == Some(message_b_uuid.as_str()))
        .ok_or_else(|| anyhow::anyhow!("message B missing from transcript: {detail}"))?;
    anyhow::ensure!(
        b_row["in_reply_to_uuid"].as_str() == Some(message_a_uuid.as_str()),
        "B must carry A's message_uuid as its in_reply_to_uuid: {b_row}"
    );

    Ok((thread_uuid, message_a_uuid, message_b_uuid))
}

async fn assert_thread_route_rejects_bad_context_kind(
    ctx: &Ctx<'_>,
    thread_uuid: &str,
) -> anyhow::Result<()> {
    let thread_bad_kind_resp = ctx
        .client
        .post(format!(
            "{}/api/threads/{thread_uuid}/message",
            ctx.inspect_url
        ))
        .json(&json!({
            "body": "should never land",
            "context": [{"kind": "bogus", "value": "x"}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        thread_bad_kind_resp.status() == 400,
        "unknown context kind must 400 on the thread route, got {}",
        thread_bad_kind_resp.status()
    );
    let thread_bad_kind_body: Value = thread_bad_kind_resp.json().await?;
    anyhow::ensure!(
        thread_bad_kind_body["error"].as_str() == Some("invalid_context_kind"),
        "error code must be invalid_context_kind: {thread_bad_kind_body}"
    );
    Ok(())
}

async fn assert_cross_thread_reply_rejected(
    ctx: &Ctx<'_>,
    alice: &str,
    bob: &str,
    thread_uuid: &str,
) -> anyhow::Result<()> {
    let other_thread_resp = ctx
        .client
        .post(format!("{}/api/sessions/{bob}/message", ctx.inspect_url))
        .json(&json!({
            "message": "an unrelated thread's root",
            "from_session_id": alice,
            "subject": "a completely different thread",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        other_thread_resp.status() == 201,
        "other-thread create returned {}",
        other_thread_resp.status()
    );
    let other_thread_body: Value = other_thread_resp.json().await?;
    let other_message_uuid = other_thread_body["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {other_thread_body}"))?
        .to_string();

    let wrong_thread_resp = ctx
        .client
        .post(format!(
            "{}/api/threads/{thread_uuid}/message",
            ctx.inspect_url
        ))
        .json(&json!({
            "body": "should be rejected — wrong thread",
            "in_reply_to": other_message_uuid,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        wrong_thread_resp.status() == 400,
        "in_reply_to naming a message from another thread must 400, got {}",
        wrong_thread_resp.status()
    );
    let wrong_thread_body: Value = wrong_thread_resp.json().await?;
    anyhow::ensure!(
        wrong_thread_body["error"].as_str() == Some("invalid_in_reply_to"),
        "error code must be invalid_in_reply_to: {wrong_thread_body}"
    );
    Ok(())
}

async fn assert_reply_chain_nests_to_depth_three(
    ctx: &Ctx<'_>,
    alice: &str,
    thread_uuid: &str,
    message_a_uuid: &str,
    message_b_uuid: &str,
) -> anyhow::Result<()> {
    let message_c_resp = ctx
        .client
        .post(format!(
            "{}/api/threads/{thread_uuid}/message",
            ctx.inspect_url
        ))
        .json(&json!({
            "body": "message C — replies to B",
            "from_session_id": alice,
            "in_reply_to": message_b_uuid,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        message_c_resp.status() == 201,
        "message C send returned {}",
        message_c_resp.status()
    );
    let message_c_body: Value = message_c_resp.json().await?;
    let message_c_uuid = message_c_body["deliveries"][0]["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing delivery message_uuid: {message_c_body}"))?
        .to_string();

    let final_detail_resp = ctx
        .client
        .get(format!("{}/api/threads/{thread_uuid}", ctx.inspect_url))
        .send()
        .await?;
    let final_detail: Value = final_detail_resp.json().await?;
    let final_messages = final_detail["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {final_detail}"))?;
    anyhow::ensure!(
        final_messages.len() == 3,
        "the thread has exactly A, B, C: {final_detail}"
    );
    let find_row = |uuid: &str| -> anyhow::Result<&Value> {
        final_messages
            .iter()
            .find(|r| r["message_uuid"].as_str() == Some(uuid))
            .ok_or_else(|| anyhow::anyhow!("row {uuid} missing from transcript: {final_detail}"))
    };
    let row_a = find_row(message_a_uuid)?;
    let row_b = find_row(message_b_uuid)?;
    let row_c = find_row(&message_c_uuid)?;
    anyhow::ensure!(
        row_a["in_reply_to_uuid"].is_null(),
        "A is the thread root — no parent: {row_a}"
    );
    anyhow::ensure!(
        row_b["in_reply_to_uuid"].as_str() == Some(message_a_uuid),
        "B replies to A: {row_b}"
    );
    anyhow::ensure!(
        row_c["in_reply_to_uuid"].as_str() == Some(message_b_uuid),
        "C replies to B — depth 3 (A ← B ← C) nests correctly: {row_c}"
    );
    Ok(())
}

#[tokio::test]
async fn message_context_items_persist_round_trip_and_reply_chains_nest() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_message_context");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let alice = "dd000000-0000-4000-d000-000000290001";
    let bob = "dd000000-0000-4000-d000-000000290002";
    ensure_session(&pool, alice, "mailbox-context-test").await?;
    ensure_session(&pool, bob, "mailbox-context-test").await?;

    let client = reqwest::Client::new();
    let ctx = Ctx {
        client: &client,
        inspect_url: &inspect_url,
    };

    assert_session_route_context_round_trips(&ctx, bob, alice).await?;
    assert_bare_send_has_empty_context_items(&ctx, bob).await?;
    assert_session_route_rejects_bad_context_kind(&ctx, bob).await?;

    let (thread_uuid, message_a_uuid, message_b_uuid) =
        create_thread_with_context_reply(&ctx, &pool, alice, bob).await?;
    assert_thread_route_rejects_bad_context_kind(&ctx, &thread_uuid).await?;
    assert_cross_thread_reply_rejected(&ctx, alice, bob, &thread_uuid).await?;
    assert_reply_chain_nests_to_depth_three(
        &ctx,
        alice,
        &thread_uuid,
        &message_a_uuid,
        &message_b_uuid,
    )
    .await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
