use std::time::Duration;

use axum::{Router, body::Body, response::IntoResponse, routing::any};
use bytes::Bytes;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::db_helpers;

const CSE: &str = "cse_PushWakeTest";

async fn upstream() -> anyhow::Result<(String, tokio::task::JoinHandle<()>)> {
    let app = Router::new().route(
        "/{*path}",
        any(|method: axum::http::Method| async move {
            if method == axum::http::Method::GET {
                let stream = futures::stream::once(async {
                    Ok::<_, std::io::Error>(Bytes::from_static(b": ready\n\n"))
                })
                .chain(futures::stream::pending());
                (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(stream),
                )
                    .into_response()
            } else {
                (axum::http::StatusCode::BAD_REQUEST, "unknown event").into_response()
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((url, task))
}

async fn register(client: &reqwest::Client, base: &str, id: Uuid) -> anyhow::Result<()> {
    client.post(format!("{base}/api/workspace/register"))
        .json(&json!({"session_id": id, "cwd": "/tmp/rc-push-wake", "repo_root": "/tmp/rc-push-wake"}))
        .send().await?.error_for_status()?;
    Ok(())
}

async fn send(
    client: &reqwest::Client,
    base: &str,
    sender: Uuid,
    recipient: Uuid,
    extra: Value,
) -> anyhow::Result<Uuid> {
    let mut body =
        json!({"message": "Wake up and inspect the result — 雪", "from_session_id": sender});
    if let Some(extra) = extra.as_object() {
        body.as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("body"))?
            .extend(extra.clone());
    }
    let response = client
        .post(format!("{base}/api/sessions/{recipient}/message"))
        .json(&body)
        .send()
        .await?;
    assert_eq!(response.status().as_u16(), 201);
    let value: Value = response.json().await?;
    Ok(value["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{value}"))?
        .parse()?)
}

async fn ack(
    client: &reqwest::Client,
    base: &str,
    cse: &str,
    event: Uuid,
    status: &str,
) -> anyhow::Result<()> {
    let response = client
        .post(format!(
            "{base}/v1/code/sessions/{cse}/worker/events/delivery"
        ))
        .json(&json!({"worker_epoch": 1, "updates": [{"event_id": event, "status": status}]}))
        .send()
        .await?;
    assert_eq!(response.status().as_u16(), 400);
    Ok(())
}

async fn wait_ack(pool: &sqlx::PgPool, event: Uuid) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let acked: bool = sqlx::query_scalar(
                "SELECT acked_at IS NOT NULL FROM remote_control.injections WHERE event_id=$1",
            )
            .bind(event)
            .fetch_one(pool)
            .await?;
            if acked {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    Ok(())
}

async fn assert_receipts(
    client: &reqwest::Client,
    base: &str,
    sender: Uuid,
    recipient: Uuid,
    message: Uuid,
) -> anyhow::Result<()> {
    let value: Value = client
        .get(format!(
            "{base}/api/sessions/{sender}/messages/{message}/receipt"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(value["status"], "consumed", "{value}");
    assert!(!value["consumed_at"].is_null());
    assert!(
        value["sent_request_id"].is_null(),
        "never invent an inference request ID"
    );
    let evidence: Value = client
        .get(format!(
            "{base}/api/sessions/{recipient}/messages/{message}/receipt-evidence"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(evidence["confidence"], "acknowledged", "{evidence}");
    Ok(())
}

#[tokio::test]
async fn mailbox_wakes_through_real_relay_and_correlates_client_receipt() -> anyhow::Result<()> {
    let (upstream_url, upstream_task) = upstream().await?;
    let handle = db_helpers::start_proxy_with_shared_db(&upstream_url)
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL is required"))?;
    let client = reqwest::Client::new();
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    register(&client, &handle.inspect_url, sender).await?;
    register(&client, &handle.inspect_url, recipient).await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid, worker_status) VALUES ($1,$2,'idle')")
        .bind(CSE).bind(recipient).execute(&handle.pool).await?;
    let response = client
        .get(format!(
            "{}/v1/code/sessions/{CSE}/worker/events/stream",
            handle.proxy_url
        ))
        .send()
        .await?;
    let mut stream = response.bytes_stream();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await?
            .is_some()
    );

    for extra in [json!({}), json!({"subject": "push-wake regression"})] {
        let started = tokio::time::Instant::now();
        let message = send(&client, &handle.inspect_url, sender, recipient, extra).await?;
        let frame = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("stream closed"))??;
        assert!(started.elapsed() < Duration::from_secs(5));
        let text = std::str::from_utf8(&frame)?;
        assert!(text.starts_with("event: client_event\n"), "{text}");
        assert!(!text.lines().any(|line| line.starts_with("id:")));
        let data: Value = serde_json::from_str(
            text.lines()
                .find_map(|line| line.strip_prefix("data: "))
                .ok_or_else(|| anyhow::anyhow!("no data"))?,
        )?;
        assert_eq!(data["event_type"], "user");
        assert_eq!(data["payload"]["type"], "user");
        assert!(
            data["payload"]["message"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("Wake up and inspect the result — 雪")
        );
        let event: Uuid = data["event_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no ID"))?
            .parse()?;
        let stored: (Vec<u8>, String, bool, bool) = sqlx::query_as("SELECT frame, authored_by, emitted_at IS NOT NULL, acked_at IS NOT NULL FROM remote_control.injections WHERE event_id=$1")
            .bind(event).fetch_one(&handle.pool).await?;
        assert_eq!(stored, (frame.to_vec(), "mike".into(), true, false));
        ack(
            &client,
            &handle.proxy_url,
            "cse_WrongRecipient",
            event,
            "received",
        )
        .await?;
        ack(&client, &handle.proxy_url, CSE, event, "failed").await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let consumed: bool = sqlx::query_scalar(
            "SELECT consumed_at IS NOT NULL FROM mailbox.messages WHERE message_uuid=$1",
        )
        .bind(message)
        .fetch_one(&handle.pool)
        .await?;
        assert!(
            !consumed,
            "wrong-channel or non-received status cannot acknowledge a message"
        );
        ack(&client, &handle.proxy_url, CSE, event, "received").await?;
        wait_ack(&handle.pool, event).await?;
        assert_receipts(&client, &handle.inspect_url, sender, recipient, message).await?;
        ack(&client, &handle.proxy_url, CSE, event, "received").await?;
        sqlx::query("UPDATE remote_control.channels SET worker_status='running' WHERE cse_id=$1")
            .bind(CSE)
            .execute(&handle.pool)
            .await?;
    }
    drop(stream);
    handle.shutdown.cancel();
    upstream_task.abort();
    handle.pool.close().await;
    handle.test_db.cleanup().await?;
    Ok(())
}

async fn next_event<S>(stream: &mut S, buffered: &mut String) -> anyhow::Result<String>
where
    S: futures::Stream<Item = reqwest::Result<Bytes>> + Unpin,
{
    loop {
        if let Some(end) = buffered.find("\n\n") {
            let event = buffered[..end + 2].to_owned();
            buffered.drain(..end + 2);
            return Ok(event);
        }
        let chunk = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("stream closed"))??;
        buffered.push_str(std::str::from_utf8(&chunk)?);
    }
}

fn event_text(event: &str) -> anyhow::Result<String> {
    anyhow::ensure!(event.starts_with("event: client_event\n"), "{event}");
    let data: Value = serde_json::from_str(
        event
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .ok_or_else(|| anyhow::anyhow!("no data"))?,
    )?;
    Ok(data["payload"]["message"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

#[tokio::test]
#[ignore]
async fn thread_reply_pushes_through_real_relay() -> anyhow::Result<()> {
    const REPLY_CSE: &str = "cse_ThreadReplyPushTest";
    let (upstream_url, upstream_task) = upstream().await?;
    let handle = db_helpers::start_proxy_with_shared_db(&upstream_url)
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL is required"))?;
    let client = reqwest::Client::new();
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    register(&client, &handle.inspect_url, sender).await?;
    register(&client, &handle.inspect_url, recipient).await?;

    send(
        &client,
        &handle.inspect_url,
        sender,
        recipient,
        json!({"subject": "thread reply push regression"}),
    )
    .await?;
    let thread_uuid: Uuid = sqlx::query_scalar(
        "SELECT thread_uuid FROM mailbox.threads WHERE subject = 'thread reply push regression'",
    )
    .fetch_one(&handle.pool)
    .await?;

    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid, worker_status) VALUES ($1,$2,'idle')")
        .bind(REPLY_CSE).bind(recipient).execute(&handle.pool).await?;
    let response = client
        .get(format!(
            "{}/v1/code/sessions/{REPLY_CSE}/worker/events/stream",
            handle.proxy_url
        ))
        .send()
        .await?;
    let mut stream = response.bytes_stream();
    let mut buffered = String::new();
    let mut opening_events = vec![
        next_event(&mut stream, &mut buffered).await?,
        next_event(&mut stream, &mut buffered).await?,
    ];
    let ready_at = opening_events
        .iter()
        .position(|event| event == ": ready\n\n")
        .ok_or_else(|| anyhow::anyhow!("no ready comment in {opening_events:?}"))?;
    opening_events.remove(ready_at);
    let opening = event_text(&opening_events[0])?;
    assert!(
        opening.contains("Wake up and inspect the result — 雪"),
        "{opening}"
    );

    let reply = client
        .post(format!(
            "{}/api/threads/{thread_uuid}/message",
            handle.inspect_url
        ))
        .json(&json!({
            "from_session_id": sender,
            "body": "reply while you were idle — 雪",
        }))
        .send()
        .await?;
    assert_eq!(reply.status().as_u16(), 201);

    let text = event_text(&next_event(&mut stream, &mut buffered).await?)?;
    assert!(text.contains("reply while you were idle — 雪"), "{text}");

    drop(stream);
    handle.shutdown.cancel();
    upstream_task.abort();
    handle.pool.close().await;
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn absent_reply_stream_keeps_existing_mailbox_fallback() -> anyhow::Result<()> {
    let handle = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL is required"))?;
    let client = reqwest::Client::new();
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    register(&client, &handle.inspect_url, sender).await?;
    register(&client, &handle.inspect_url, recipient).await?;
    let message = send(&client, &handle.inspect_url, sender, recipient, json!({})).await?;
    let row: (bool, bool, i64) = sqlx::query_as("SELECT claimed_at IS NULL, sent_at IS NULL, (SELECT count(*) FROM remote_control.injections WHERE message_uuid=$1) FROM mailbox.messages WHERE message_uuid=$1")
        .bind(message).fetch_one(&handle.pool).await?;
    assert_eq!(row, (true, true, 0));
    handle.shutdown.cancel();
    handle.pool.close().await;
    handle.test_db.cleanup().await?;
    Ok(())
}
