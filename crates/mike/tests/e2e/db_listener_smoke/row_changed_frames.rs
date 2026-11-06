use super::harness::{
    ProxyHandle, insert_request, is_row_changed, next_matching, seed_project_and_session,
    start_proxy_with_listener, ws_url,
};

use anyhow::Context as _;
use futures::StreamExt;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn row_mutations_emit_scoped_row_changed_frames() -> anyhow::Result<()> {
    let Some(handle) = start_proxy_with_listener().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping db_listener_smoke");
        return Ok(());
    };

    let (mut ws, _resp) = connect_async(ws_url(&handle.inspect_url)).await?;

    let (project_id, session_id) = seed_project_and_session(&handle.pool).await?;

    assert_requests_insert_frame(&mut ws, &handle, project_id, &session_id).await?;
    assert_sessions_update_frame(&mut ws, &handle, &session_id).await?;
    assert_filter_rules_insert_frame(&mut ws, &handle).await?;
    assert_prompt_compositions_insert_frame(&mut ws, &handle).await?;
    assert_mailbox_message_insert_frame(&mut ws, &handle, &session_id).await?;
    assert_mailbox_thread_insert_and_update_frames(&mut ws, &handle).await?;

    handle.cleanup().await?;
    Ok(())
}

async fn assert_requests_insert_frame<S>(
    ws: &mut S,
    handle: &ProxyHandle,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let req_id = insert_request(&handle.pool, project_id, session_id).await?;
    let frame = next_matching(ws, is_row_changed("requests", &req_id.to_string()))
        .await
        .context("requests INSERT frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "requests");
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["id"], req_id.to_string());
    assert_eq!(frame["session_id"], session_id);
    Ok(())
}

async fn assert_sessions_update_frame<S>(
    ws: &mut S,
    handle: &ProxyHandle,
    session_id: &str,
) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    sqlx::query(
        "UPDATE sessions.sessions
            SET title = 'row changed frame'
          WHERE session_uuid = $1::text::uuid",
    )
    .bind(session_id)
    .execute(&handle.pool)
    .await?;
    let frame = next_matching(ws, is_row_changed("sessions", session_id))
        .await
        .context("sessions UPDATE frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "sessions");
    assert_eq!(frame["op"], "UPDATE");
    assert_eq!(frame["id"], session_id);
    assert_eq!(
        frame["session_id"], session_id,
        "sessions frames carry their own uuid so session-scoped consumers can scope them"
    );
    Ok(())
}

async fn assert_filter_rules_insert_frame<S>(ws: &mut S, handle: &ProxyHandle) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let rule_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO rules.rules (uuid, kind_id, value, action_id, source_id)
         SELECT $1, rk.id, 'noisy-banner', ra.id, rs.id
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'text_substring'
           AND ra.code = 'drop'
           AND rs.code = 'ui'",
    )
    .bind(rule_id)
    .execute(&handle.pool)
    .await?;
    let frame = next_matching(ws, is_row_changed("filter_rules", &rule_id.to_string()))
        .await
        .context("filter_rules INSERT frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "filter_rules");
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["id"], rule_id.to_string());
    Ok(())
}

async fn assert_prompt_compositions_insert_frame<S>(
    ws: &mut S,
    handle: &ProxyHandle,
) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let prompt_sha = "b".repeat(64);
    sqlx::query(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
         VALUES ($1, '[]'::jsonb)",
    )
    .bind(&prompt_sha)
    .execute(&handle.pool)
    .await?;
    let comp_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO overrides.overrides (prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, (SELECT id FROM system_prompts.system_prompts WHERE prompt_sha256 = $1),
             'phase-b2-test', '[]'::jsonb)
         RETURNING uuid",
    )
    .bind(&prompt_sha)
    .fetch_one(&handle.pool)
    .await?;
    let frame = next_matching(
        ws,
        is_row_changed("prompt_compositions", &comp_id.to_string()),
    )
    .await
    .context("prompt_compositions INSERT frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "prompt_compositions");
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["id"], comp_id.to_string());
    Ok(())
}

async fn assert_mailbox_message_insert_frame<S>(
    ws: &mut S,
    handle: &ProxyHandle,
    session_id: &str,
) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mailbox_message_uuid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, body)
         SELECT $1, s.id, 'db-listener-smoke test message'
           FROM sessions.sessions s WHERE s.session_uuid = $2::text::uuid",
    )
    .bind(mailbox_message_uuid)
    .bind(session_id)
    .execute(&handle.pool)
    .await?;
    let frame = next_matching(
        ws,
        is_row_changed("mailbox_messages", &mailbox_message_uuid.to_string()),
    )
    .await
    .context("mailbox.messages INSERT frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "mailbox_messages");
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["id"], mailbox_message_uuid.to_string());
    assert_eq!(frame["session_id"], session_id);
    Ok(())
}

async fn assert_mailbox_thread_insert_and_update_frames<S>(
    ws: &mut S,
    handle: &ProxyHandle,
) -> anyhow::Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mailbox_thread_uuid = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO mailbox.threads (thread_uuid, subject) VALUES ($1, $2)")
        .bind(mailbox_thread_uuid)
        .bind("db-listener-smoke test thread")
        .execute(&handle.pool)
        .await?;
    let frame = next_matching(
        ws,
        is_row_changed("mailbox_threads", &mailbox_thread_uuid.to_string()),
    )
    .await
    .context("mailbox.threads INSERT frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "mailbox_threads");
    assert_eq!(frame["op"], "INSERT");
    assert_eq!(frame["id"], mailbox_thread_uuid.to_string());
    assert!(
        frame.get("session_id").is_none_or(Value::is_null),
        "mailbox_threads notifications should not carry a non-null session_id; got {:?}",
        frame.get("session_id")
    );

    sqlx::query("UPDATE mailbox.threads SET resolved_at = now() WHERE thread_uuid = $1")
        .bind(mailbox_thread_uuid)
        .execute(&handle.pool)
        .await?;
    let frame = next_matching(ws, |v: &Value| {
        v.get("type").and_then(Value::as_str) == Some("RowChanged")
            && v.get("table").and_then(Value::as_str) == Some("mailbox_threads")
            && v.get("op").and_then(Value::as_str) == Some("UPDATE")
            && v.get("id").and_then(Value::as_str) == Some(mailbox_thread_uuid.to_string().as_str())
    })
    .await
    .context("mailbox.threads UPDATE frame")?;
    assert_eq!(frame["type"], "RowChanged");
    assert_eq!(frame["table"], "mailbox_threads");
    assert_eq!(frame["op"], "UPDATE");
    assert_eq!(frame["id"], mailbox_thread_uuid.to_string());
    Ok(())
}
