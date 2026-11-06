use crate::db_helpers;
use crate::sse;

use serde_json::{Value, json};
use sse::sse_body;
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION_HEADER: &str = "x-claude-code-session-id";

const FORBIDDEN_WORD: &str = "pre-existing";

const STREAMED_HIT_SESSION: &str = "d3b1ef00-3f4e-4c1b-9e9f-9fe2e02b6d44";

const STREAMED_PARTIAL_SESSION: &str = "a6e41233-6c71-4f4e-a1c2-c2b5b35e9077";

const CLEAN_OPENING: &str = "The build is green on every target we ship. ";

const BUFFERED_ARMED_SESSION: &str = "e4c2f011-4a5f-4d2c-8fa0-a0f3f13c7e55";

const BUFFERED_UNARMED_SESSION: &str = "f5d30122-5b60-4e3d-90b1-b1a4a24d8f66";

const CAPABILITY_MARKER: &str = "answered after the correction";

fn text_sse(text: &str) -> String {
    let delta = json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": text}
    });
    sse_body(&[
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_318"}}"#,
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ),
        ("content_block_delta", &delta.to_string()),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ])
}

fn text_sse_in_deltas(deltas: &[&str]) -> String {
    let rendered: Vec<String> = deltas
        .iter()
        .map(|text| {
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": text}
            })
            .to_string()
        })
        .collect();
    let mut events: Vec<(&str, &str)> = vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_318_partial"}}"#,
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ),
    ];
    events.extend(rendered.iter().map(|d| ("content_block_delta", d.as_str())));
    events.extend([
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ]);
    sse_body(&events)
}

fn client_frames(client_saw: &str) -> Vec<(String, Value)> {
    client_saw
        .split("\n\n")
        .filter_map(|frame| {
            let event = frame.lines().find_map(|l| l.strip_prefix("event: "))?;
            let data = frame.lines().find_map(|l| l.strip_prefix("data: "))?;
            Some((event.to_owned(), serde_json::from_str(data).ok()?))
        })
        .collect()
}

fn json_message(text: &str) -> Value {
    json!({
        "id": "msg_318_buffered",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-5",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 3, "output_tokens": 5}
    })
}

async fn mount_upstream(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, STREAMED_HIT_SESSION))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            text_sse("this is a pre-existing problem").into_bytes(),
            "text/event-stream",
        ))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, STREAMED_HIT_SESSION))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            text_sse(CAPABILITY_MARKER).into_bytes(),
            "text/event-stream",
        ))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, STREAMED_PARTIAL_SESSION))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                text_sse_in_deltas(&[CLEAN_OPENING, "It does have one pre-", "existing failure."])
                    .into_bytes(),
                "text/event-stream",
            ),
        )
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header(SESSION_HEADER, STREAMED_PARTIAL_SESSION))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            text_sse(CAPABILITY_MARKER).into_bytes(),
            "text/event-stream",
        ))
        .mount(server)
        .await;
    for session in [BUFFERED_ARMED_SESSION, BUFFERED_UNARMED_SESSION] {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header(SESSION_HEADER, session))
            .respond_with(ResponseTemplate::new(200).set_body_json(json_message(
                "a pre-existing failure, said in a buffered body",
            )))
            .mount(server)
            .await;
    }
}

async fn patch(inspect_url: &str, route: &str, body: Value) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .patch(format!("{inspect_url}{route}"))
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "PATCH {route} returned {}",
        resp.status()
    );
    Ok(())
}

async fn arm_weasel_intercept(inspect_url: &str) -> anyhow::Result<()> {
    patch(
        inspect_url,
        "/api/settings/weasel_intercept.forbidden_words",
        json!({"value": [FORBIDDEN_WORD]}),
    )
    .await?;
    patch(
        inspect_url,
        "/api/settings/features/weasel_intercept",
        json!({"enabled": true}),
    )
    .await
}

async fn drive_request(proxy_url: &str, session_id: &str, stream: bool) -> anyhow::Result<String> {
    let body = json!({
        "model": "claude-opus-4-5",
        "max_tokens": 1024,
        "stream": stream,
        "messages": [{"role": "user", "content": "how is the build?"}],
        "system": "You are a helpful assistant.",
    });
    let resp = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(SESSION_HEADER, session_id)
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    Ok(String::from_utf8_lossy(&resp.bytes().await?).to_string())
}

type EventRow = (Option<i64>, String, Option<String>);

async fn events_for(pool: &sqlx::PgPool, session_id: &str) -> anyhow::Result<Vec<EventRow>> {
    Ok(sqlx::query_as::<_, EventRow>(
        "SELECT ri.request_id, ri.outcome, ri.subject
           FROM requests.response_intercepts ri
           JOIN sessions.sessions s ON s.id = ri.session_id
          WHERE s.session_uuid = $1::text::uuid
          ORDER BY ri.id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

async fn wait_for_events(
    pool: &sqlx::PgPool,
    session_id: &str,
    want: usize,
) -> anyhow::Result<Vec<EventRow>> {
    let mut seen = Vec::new();
    for _ in 0..50 {
        seen = events_for(pool, session_id).await?;
        if seen.len() >= want {
            return Ok(seen);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("expected {want} intercept event(s) for session {session_id}, saw {seen:?}")
}

async fn intercept_flags(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<(bool, Option<i64>)> {
    Ok(sqlx::query_as::<_, (bool, Option<i64>)>(
        "SELECT intercepted, correction_request_id FROM requests.requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn streamed_intercept_leaves_a_durable_row_naming_the_word_and_the_request()
-> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    mount_upstream(&upstream).await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    arm_weasel_intercept(&proxy.inspect_url).await?;

    let client_saw = drive_request(&proxy.proxy_url, STREAMED_HIT_SESSION, true).await?;
    assert!(
        client_saw.contains(CAPABILITY_MARKER) && !client_saw.contains(FORBIDDEN_WORD),
        "the client must receive the correction answer and never the listed phrase; got:\n{client_saw}"
    );

    let events = wait_for_events(&proxy.pool, STREAMED_HIT_SESSION, 1).await?;
    assert_eq!(events.len(), 1, "{events:?}");
    let (request_id, outcome, subject) = &events[0];
    assert_eq!(outcome, "weasel_correction");
    assert_eq!(subject.as_deref(), Some(FORBIDDEN_WORD));
    let request_id = request_id.ok_or_else(|| {
        anyhow::anyhow!("the original request persisted, so the event must name it: {events:?}")
    })?;
    let (intercepted, correction) = intercept_flags(&proxy.pool, request_id).await?;
    assert!(intercepted, "requests.requests.intercepted must be true");
    assert!(
        correction.is_some(),
        "requests.requests.correction_request_id must name the correction"
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_reply_cut_at_a_listed_phrase_keeps_its_opening_and_continues_as_one_message()
-> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    mount_upstream(&upstream).await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    arm_weasel_intercept(&proxy.inspect_url).await?;

    let client_saw = drive_request(&proxy.proxy_url, STREAMED_PARTIAL_SESSION, true).await?;
    assert!(
        client_saw.contains(CLEAN_OPENING),
        "text that arrived before the phrase had already been streamed; got:\n{client_saw}"
    );
    assert!(
        !client_saw.contains(FORBIDDEN_WORD) && !client_saw.contains("one pre-"),
        "neither the phrase nor its first half may reach the client; got:\n{client_saw}"
    );

    let frames = client_frames(&client_saw);
    let kinds: Vec<&str> = frames.iter().map(|(event, _)| event.as_str()).collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "message_start").count(),
        1,
        "the corrected reply continues the message the client already has: {kinds:?}"
    );
    let stop_of_first = frames
        .iter()
        .position(|(event, data)| event == "content_block_stop" && data["index"] == 0)
        .ok_or_else(|| anyhow::anyhow!("the cut text block must be closed: {kinds:?}"))?;
    let start_of_second = frames
        .iter()
        .position(|(event, data)| event == "content_block_start" && data["index"] == 1)
        .ok_or_else(|| anyhow::anyhow!("the correction must open block 1: {kinds:?}"))?;
    assert!(stop_of_first < start_of_second, "{kinds:?}");
    let correction = frames
        .iter()
        .find(|(_, data)| data["delta"]["text"] == CAPABILITY_MARKER)
        .ok_or_else(|| anyhow::anyhow!("the correction text must be streamed: {kinds:?}"))?;
    assert_eq!(correction.1["index"], 1);
    assert_eq!(kinds.last(), Some(&"message_stop"), "{kinds:?}");

    let received = upstream.received_requests().await.unwrap_or_default();
    let correction_request: Value = serde_json::from_slice(
        &received
            .last()
            .ok_or_else(|| anyhow::anyhow!("no correction request reached upstream"))?
            .body,
    )?;
    let messages = correction_request["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("correction request has no messages"))?;
    let cut_reply = messages[messages.len() - 2]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        cut_reply.starts_with(CLEAN_OPENING) && cut_reply.contains(FORBIDDEN_WORD),
        "the correction request quotes the reply up to the phrase; got: {cut_reply}"
    );

    let events = wait_for_events(&proxy.pool, STREAMED_PARTIAL_SESSION, 1).await?;
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].1, "weasel_correction");

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn buffered_response_is_durably_marked_not_scanned_only_when_armed() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    mount_upstream(&upstream).await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };

    drive_request(&proxy.proxy_url, BUFFERED_UNARMED_SESSION, false).await?;
    arm_weasel_intercept(&proxy.inspect_url).await?;
    let body = drive_request(&proxy.proxy_url, BUFFERED_ARMED_SESSION, false).await?;
    assert!(
        body.contains(FORBIDDEN_WORD),
        "a buffered body passes through unscanned; got:\n{body}"
    );

    let events = wait_for_events(&proxy.pool, BUFFERED_ARMED_SESSION, 1).await?;
    assert_eq!(events.len(), 1, "{events:?}");
    let (request_id, outcome, subject) = &events[0];
    assert_eq!(outcome, "not_scanned_buffered");
    assert_eq!(*subject, None);
    assert!(
        request_id.is_some(),
        "the buffered request persisted, so the marker must name it: {events:?}"
    );
    assert!(
        events_for(&proxy.pool, BUFFERED_UNARMED_SESSION)
            .await?
            .is_empty(),
        "a session that was not armed at its first request must leave no marker"
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capabilities_reports_the_live_forbidden_words_not_the_boot_config() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    arm_weasel_intercept(&proxy.inspect_url).await?;

    let payload: Value = reqwest::Client::new()
        .get(format!("{}/api/capabilities", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let row = payload["capabilities"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == "weasel_intercept"))
        .ok_or_else(|| anyhow::anyhow!("no weasel_intercept row in {payload}"))?;
    assert_eq!(
        row["state"], "active",
        "the live overlay holds one forbidden word; got {row}"
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}
