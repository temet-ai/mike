use crate::db_helpers;

use serde_json::Value;
use sqlx::PgPool;

async fn make_session(pool: &PgPool, session_uuid: &str) -> anyhow::Result<i64> {
    let project = "receipt-fixture-project";
    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, now(), now()
          WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)",
    )
    .bind(project)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(project)
    .fetch_one(pool)
    .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, now(), now())
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = now()
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn queue_message(
    pool: &PgPool,
    message_uuid: &str,
    to_session_db_id: i64,
    body: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mailbox.messages
            (message_uuid, to_session_id, body, queued_at, delivery_attempts)
         VALUES ($1::text::uuid, $2, $3, now() - interval '1 hour', 0)",
    )
    .bind(message_uuid)
    .bind(to_session_db_id)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

async fn capture_request(
    pool: &PgPool,
    session_db_id: i64,
    text: &str,
    minutes_ago: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body, created_at
         )
         SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 0, false,
                rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                convert_to($2, 'UTF8'), now() - ($3 || ' minutes')::interval
           FROM sessions.sessions s,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.id = $1
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
    )
    .bind(session_db_id)
    .bind(text)
    .bind(minutes_ago.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

async fn get_receipt(
    inspect_url: &str,
    session_uuid: &str,
    message_uuid: &str,
) -> anyhow::Result<Value> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/sessions/{session_uuid}/messages/{message_uuid}/receipt-evidence"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200, "receipt lookup must succeed");
    Ok(resp.json().await?)
}

#[tokio::test]
async fn receipt_reports_high_when_the_row_says_undelivered_but_the_recipient_cited_it()
-> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let recipient = "c0000000-0000-4000-8000-000000000001";
    let msg = "d0000000-0000-4000-8000-000000000001";
    let recipient_db_id = make_session(&pool, recipient).await?;
    queue_message(
        &pool,
        msg,
        recipient_db_id,
        "ruling: land the migration first",
    )
    .await?;

    capture_request(
        &pool,
        recipient_db_id,
        &format!("acting on ruling {msg} now"),
        30,
    )
    .await?;

    let body = get_receipt(&inspect_url, recipient, msg).await?;

    assert_eq!(
        body["confidence"].as_str(),
        Some("high"),
        "a uuid citation in the recipient's own traffic is receipt, whatever the row says: {body}"
    );
    assert_eq!(body["delivery_row"]["delivery_attempts"].as_i64(), Some(0));
    assert!(body["delivery_row"]["claimed_at"].is_null());
    assert!(body["delivery_row"]["consumed_at"].is_null());

    let evidence = body["receipt_evidence"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("receipt_evidence must be an array"))?;
    assert_eq!(evidence.len(), 1, "one citing request: {body}");
    assert_eq!(evidence[0]["match_kind"].as_str(), Some("message_uuid"));
    assert!(
        evidence[0]["excerpt"]
            .as_str()
            .is_some_and(|e| e.contains(msg)),
        "the excerpt must show the citation in context: {body}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn receipt_reports_none_on_silence_but_never_calls_it_non_receipt() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let recipient = "c0000000-0000-4000-8000-000000000002";
    let sender = "c0000000-0000-4000-8000-000000000003";
    let msg = "d0000000-0000-4000-8000-000000000002";
    let recipient_db_id = make_session(&pool, recipient).await?;
    let sender_db_id = make_session(&pool, sender).await?;
    queue_message(
        &pool,
        msg,
        recipient_db_id,
        "a distinctive body nobody echoed back",
    )
    .await?;

    capture_request(&pool, sender_db_id, &format!("sending {msg} now"), 30).await?;
    capture_request(&pool, recipient_db_id, "unrelated work", 20).await?;

    let body = get_receipt(&inspect_url, recipient, msg).await?;

    assert_eq!(
        body["confidence"].as_str(),
        Some("none"),
        "the sender's own copy is not the recipient's receipt: {body}"
    );
    assert_eq!(
        body["receipt_evidence"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("receipt_evidence must be an array"))?
            .len(),
        0
    );
    assert!(
        body["caveat"]
            .as_str()
            .is_some_and(|c| c.contains("not evidence of non-receipt")),
        "an empty result must carry its own caveat: {body}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn receipt_prefers_an_explicit_ack_over_inferred_evidence() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let recipient = "c0000000-0000-4000-8000-000000000004";
    let msg = "d0000000-0000-4000-8000-000000000004";
    let recipient_db_id = make_session(&pool, recipient).await?;
    queue_message(&pool, msg, recipient_db_id, "acked message").await?;
    sqlx::query(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1::text::uuid",
    )
    .bind(msg)
    .execute(&pool)
    .await?;

    let body = get_receipt(&inspect_url, recipient, msg).await?;
    assert_eq!(
        body["confidence"].as_str(),
        Some("acknowledged"),
        "the recipient said so; that outranks anything inferred: {body}"
    );

    let stranger = "c0000000-0000-4000-8000-00000000000f";
    let resp = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/sessions/{stranger}/messages/{msg}/receipt-evidence"
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "a real message cannot be laundered as addressed to a different session"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
