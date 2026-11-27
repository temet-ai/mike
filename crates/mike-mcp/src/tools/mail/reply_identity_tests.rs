use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

async fn start_mock() -> (MockServer, InspectClient) {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    (server, client)
}

fn find_body(matches: &[(u64, &str)]) -> Value {
    let matches: Vec<Value> = matches
        .iter()
        .map(|(request_id, session_id)| {
            serde_json::json!({"request_id": request_id, "session_id": session_id})
        })
        .collect();
    serde_json::json!({ "matches": matches })
}

fn calls_body(name: &str, message: &str) -> Value {
    serde_json::json!({
        "calls": [{"name": name, "input": {"message": message, "to": "x"}}]
    })
}

async fn mount_find(server: &MockServer, matches: &[(u64, &str)]) {
    Mock::given(method("GET"))
        .and(path("/api/body/find"))
        .respond_with(ResponseTemplate::new(200).set_body_json(find_body(matches)))
        .mount(server)
        .await;
}

async fn mount_calls(server: &MockServer, request_id: u64, name: &str, message: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/requests/{request_id}/tool-calls")))
        .respond_with(ResponseTemplate::new(200).set_body_json(calls_body(name, message)))
        .mount(server)
        .await;
}

const SEND: &str = "mcp__mike__send_session_message";

#[test]
fn is_session_uuid_accepts_a_real_uuid_and_rejects_a_process_looking_string() {
    assert!(is_session_uuid("11111111-1111-4111-8111-111111111111"));
    assert!(!is_session_uuid("not-a-uuid"));
    assert!(!is_session_uuid(""));
}

#[test]
fn escaped_needle_escapes_quotes_and_truncates_at_a_char_boundary() {
    assert_eq!(
        escaped_needle("say \"hi\"").as_deref(),
        Some("say \\\"hi\\\"")
    );

    let long = "a".repeat(RESOLVE_NEEDLE_MAX_BYTES + 50);
    assert_eq!(
        escaped_needle(&long).map(|needle| needle.len()),
        Some(RESOLVE_NEEDLE_MAX_BYTES)
    );

    assert!(escaped_needle("").is_none());
}

#[test]
fn rfc3339_utc_formats_known_instants() {
    assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(rfc3339_utc(1_790_162_253), "2026-09-23T11:17:33Z");
}

#[test]
fn is_send_carrying_needs_the_send_tool_and_the_exact_message() {
    let exact = serde_json::json!({"name": SEND, "input": {"message": "hello"}});
    assert!(is_send_carrying(&exact, "hello"));
    let other_tool =
        serde_json::json!({"name": "mcp__mike__find_text", "input": {"message": "hello"}});
    assert!(!is_send_carrying(&other_tool, "hello"));
    let longer = serde_json::json!({"name": SEND, "input": {"message": "hello there"}});
    assert!(!is_send_carrying(&longer, "hello"));
}

#[test]
fn resolve_explicit_from_matches_sends_verified() -> anyhow::Result<()> {
    let resolution = resolve_explicit_from("same", &["same".to_string()])?;
    assert_eq!(resolution.from.as_deref(), Some("same"));
    assert!(!resolution.from_unverified);
    Ok(())
}

#[test]
fn resolve_explicit_from_single_mismatch_is_refused_naming_both_ids() -> anyhow::Result<()> {
    let Err(err) = resolve_explicit_from("given", &["resolved".to_string()]) else {
        anyhow::bail!("a mismatched from must be refused, not sent");
    };
    assert!(err.message.contains("given"));
    assert!(err.message.contains("resolved"));
    assert!(err.message.starts_with("Refused:"));
    Ok(())
}

#[test]
fn resolve_explicit_from_given_among_several_senders_is_verified() -> anyhow::Result<()> {
    let confirmed = ["other".to_string(), "given".to_string()];
    let resolution = resolve_explicit_from("given", &confirmed)?;
    assert!(!resolution.from_unverified);
    Ok(())
}

#[test]
fn resolve_explicit_from_ambiguous_or_unresolved_sends_unverified() -> anyhow::Result<()> {
    let ambiguous = ["a".to_string(), "b".to_string()];
    assert!(resolve_explicit_from("given", &ambiguous)?.from_unverified);
    assert!(resolve_explicit_from("given", &[])?.from_unverified);
    Ok(())
}

#[tokio::test]
async fn resolve_from_fills_the_sender_without_being_asked() -> anyhow::Result<()> {
    let (server, client) = start_mock().await;
    mount_find(&server, &[(7, "resolved-session")]).await;
    mount_calls(&server, 7, SEND, "hi").await;
    let resolution = resolve_from(&client, "hi", None, None).await?;
    assert_eq!(resolution.from.as_deref(), Some("resolved-session"));
    Ok(())
}

#[tokio::test]
async fn resolve_from_never_sends_without_a_sender() -> anyhow::Result<()> {
    let (server, client) = start_mock().await;
    mount_find(&server, &[]).await;
    match resolve_from(&client, "hi", None, None).await {
        Ok(resolution) => assert!(resolution.from.is_some() && resolution.from_unverified),
        Err(err) => assert!(err.message.contains("unknown sender")),
    }
    Ok(())
}

#[tokio::test]
async fn resolve_from_refuses_a_long_message_sent_under_the_wrong_from() -> anyhow::Result<()> {
    let (server, client) = start_mock().await;
    let message = "x".repeat(2_000);
    mount_find(&server, &[(7, "real-session")]).await;
    mount_calls(&server, 7, SEND, &message).await;

    let Err(err) = resolve_from(&client, &message, Some("process-id"), None).await else {
        anyhow::bail!("a long message under the wrong from must still be refused");
    };
    assert!(err.message.contains("process-id"));
    assert!(err.message.contains("You are real-session"));
    Ok(())
}

#[tokio::test]
async fn resolve_from_ignores_a_match_whose_tool_call_is_not_this_send() -> anyhow::Result<()> {
    let (server, client) = start_mock().await;
    mount_find(&server, &[(7, "quoting-session")]).await;
    mount_calls(&server, 7, "mcp__mike__find_text", "hi").await;

    let resolution = resolve_from(&client, "hi", Some("given"), None).await?;
    assert_eq!(resolution.from.as_deref(), Some("given"));
    assert!(resolution.from_unverified);
    Ok(())
}

#[tokio::test]
async fn resolve_from_does_not_refuse_when_two_sessions_sent_the_same_text() -> anyhow::Result<()> {
    let (server, client) = start_mock().await;
    mount_find(&server, &[(7, "a"), (8, "b")]).await;
    mount_calls(&server, 7, SEND, "ack").await;
    mount_calls(&server, 8, SEND, "ack").await;

    let resolution = resolve_from(&client, "ack", Some("c"), None).await?;
    assert!(resolution.from_unverified);
    Ok(())
}
