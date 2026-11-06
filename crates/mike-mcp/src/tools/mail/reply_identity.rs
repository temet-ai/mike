use std::time::Duration;

use rmcp::ErrorData as RmcpError;
use serde_json::Value;

use crate::client::InspectClient;

#[cfg(not(test))]
const RESOLVE_RETRY_BUDGET: Duration = Duration::from_secs(3);
#[cfg(test)]
const RESOLVE_RETRY_BUDGET: Duration = Duration::from_millis(50);
#[cfg(not(test))]
const RESOLVE_RETRY_INTERVAL: Duration = Duration::from_millis(400);
#[cfg(test)]
const RESOLVE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
const RESOLVE_LOOKBACK_SECS: u64 = 120;
const RESOLVE_NEEDLE_MAX_BYTES: usize = 300;
const RESOLVE_MAX_MATCHES: u32 = 10;
const TOOL_NAME_MARKER: &str = "send_session_message";

#[derive(Debug)]
pub(super) struct ReplyResolution {
    pub(super) from: Option<String>,
    pub(super) from_unverified: bool,
}

pub(super) async fn resolve_from(
    client: &InspectClient,
    message: &str,
    explicit_from: Option<&str>,
    request_reply: bool,
) -> Result<ReplyResolution, RmcpError> {
    let asked = explicit_from.is_some() || request_reply;
    let confirmed = resolve_via_tool_use(client, message).await;

    if let Some(from) = explicit_from {
        return resolve_explicit_from(from, &confirmed);
    }

    if let [session_id] = confirmed.as_slice() {
        return Ok(ReplyResolution {
            from: Some(session_id.clone()),
            from_unverified: false,
        });
    }

    if let Some(env_id) = env_session_id().filter(|id| is_session_uuid(id))
        && env_session_has_main_class_traffic(client, &env_id).await
    {
        return Ok(ReplyResolution {
            from: Some(env_id),
            from_unverified: true,
        });
    }

    if !asked {
        return Ok(ReplyResolution {
            from: None,
            from_unverified: false,
        });
    }
    Err(RmcpError::invalid_params(
        "request_reply is true but this session's identity could not be resolved: no \
         single session's captured send_session_message tool_use carries this exact \
         message, and this process's CLAUDE_CODE_SESSION_ID is missing, malformed, or \
         belongs to a session with no primary-turn activity. Pass `from` explicitly with \
         your own session UUID.",
        None,
    ))
}

fn resolve_explicit_from(given: &str, confirmed: &[String]) -> Result<ReplyResolution, RmcpError> {
    if confirmed.iter().any(|id| id == given) {
        return Ok(ReplyResolution {
            from: Some(given.to_string()),
            from_unverified: false,
        });
    }
    if let [resolved_id] = confirmed {
        return Err(refusal_error(given, resolved_id));
    }
    Ok(ReplyResolution {
        from: Some(given.to_string()),
        from_unverified: true,
    })
}

fn refusal_error(given: &str, resolved: &str) -> RmcpError {
    RmcpError::invalid_params(
        format!(
            "Refused: `from` is {given}, but this call came from session {resolved}. You are \
             {resolved}. Resend with from={resolved}, or pass request_reply: true and omit \
             `from`."
        ),
        None,
    )
}

fn env_session_id() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID").ok()
}

fn is_session_uuid(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

async fn resolve_via_tool_use(client: &InspectClient, message: &str) -> Vec<String> {
    let Some(needle) = escaped_needle(message) else {
        return Vec::new();
    };
    let since = lookback_rfc3339();
    let deadline = tokio::time::Instant::now() + RESOLVE_RETRY_BUDGET;
    loop {
        let confirmed = confirmed_senders(client, &needle, &since, message).await;
        if !confirmed.is_empty() || tokio::time::Instant::now() >= deadline {
            return confirmed;
        }
        tokio::time::sleep(RESOLVE_RETRY_INTERVAL).await;
    }
}

fn escaped_needle(message: &str) -> Option<String> {
    let prefix = safe_prefix(message, RESOLVE_NEEDLE_MAX_BYTES);
    if prefix.is_empty() {
        return None;
    }
    let quoted = Value::String(prefix.to_string()).to_string();
    Some(quoted[1..quoted.len().saturating_sub(1)].to_string())
}

fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn lookback_rfc3339() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    rfc3339_utc(now.saturating_sub(RESOLVE_LOOKBACK_SECS))
}

fn rfc3339_utc(epoch_secs: u64) -> String {
    let days = i64::try_from(epoch_secs / 86_400).unwrap_or(0);
    let secs_of_day = epoch_secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3_600,
        secs_of_day % 3_600 / 60,
        secs_of_day % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

async fn confirmed_senders(
    client: &InspectClient,
    needle: &str,
    since: &str,
    message: &str,
) -> Vec<String> {
    let mut confirmed: Vec<String> = Vec::new();
    for (request_id, session_id) in candidate_requests(client, needle, since).await {
        if confirmed.contains(&session_id) {
            continue;
        }
        let Ok(calls) = client.get_request_tool_calls(request_id).await else {
            continue;
        };
        if calls_carry_send(&calls, message) {
            confirmed.push(session_id);
        }
    }
    confirmed
}

async fn candidate_requests(
    client: &InspectClient,
    needle: &str,
    since: &str,
) -> Vec<(u64, String)> {
    let query = [
        ("needle", needle.to_string()),
        ("source", "response".to_string()),
        ("date_from", since.to_string()),
        ("max_matches", RESOLVE_MAX_MATCHES.to_string()),
        ("context_bytes", "0".to_string()),
    ];
    let Ok(value) = client.get_json("/api/body/find", &query).await else {
        return Vec::new();
    };
    let mut candidates: Vec<(u64, String)> = Vec::new();
    for m in value
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(pair) = match_request_and_session(m)
            && !candidates.contains(&pair)
        {
            candidates.push(pair);
        }
    }
    candidates
}

fn match_request_and_session(m: &Value) -> Option<(u64, String)> {
    let request_id = m.get("request_id")?.as_u64()?;
    let session_id = m.get("session_id")?.as_str()?.to_string();
    Some((request_id, session_id))
}

fn calls_carry_send(calls: &Value, message: &str) -> bool {
    calls
        .get("calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|call| is_send_carrying(call, message))
}

fn is_send_carrying(call: &Value, message: &str) -> bool {
    let named_send = call
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.ends_with(TOOL_NAME_MARKER));
    let same_message = call
        .get("input")
        .and_then(|input| input.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|m| m == message);
    named_send && same_message
}

async fn env_session_has_main_class_traffic(client: &InspectClient, session_id: &str) -> bool {
    let Ok(head) = client.get_session_head(session_id).await else {
        return false;
    };
    let has_requests = head.get("newest_request_id").is_some_and(|v| !v.is_null());
    let has_operator_turns = head
        .get("prompt_kind_counts")
        .and_then(|m| m.get("operator"))
        .and_then(Value::as_i64)
        .is_some_and(|n| n > 0);
    has_requests && has_operator_turns
}

#[cfg(test)]
mod tests {
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
        let resolution = resolve_from(&client, "hi", None, false).await?;
        assert_eq!(resolution.from.as_deref(), Some("resolved-session"));
        Ok(())
    }

    #[tokio::test]
    async fn resolve_from_sends_without_a_sender_when_none_resolves_and_none_was_asked()
    -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        mount_find(&server, &[]).await;
        let resolution = resolve_from(&client, "hi", None, false).await?;
        assert!(resolution.from.is_none() || resolution.from_unverified);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_from_refuses_a_long_message_sent_under_the_wrong_from() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let message = "x".repeat(2_000);
        mount_find(&server, &[(7, "real-session")]).await;
        mount_calls(&server, 7, SEND, &message).await;

        let Err(err) = resolve_from(&client, &message, Some("process-id"), false).await else {
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

        let resolution = resolve_from(&client, "hi", Some("given"), false).await?;
        assert_eq!(resolution.from.as_deref(), Some("given"));
        assert!(resolution.from_unverified);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_from_does_not_refuse_when_two_sessions_sent_the_same_text()
    -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        mount_find(&server, &[(7, "a"), (8, "b")]).await;
        mount_calls(&server, 7, SEND, "ack").await;
        mount_calls(&server, 8, SEND, "ack").await;

        let resolution = resolve_from(&client, "ack", Some("c"), false).await?;
        assert!(resolution.from_unverified);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_from_request_reply_fills_from_the_confirmed_sender() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        mount_find(&server, &[(7, "resolved-session")]).await;
        mount_calls(&server, 7, SEND, "hi").await;

        let resolution = resolve_from(&client, "hi", None, true).await?;
        assert_eq!(resolution.from.as_deref(), Some("resolved-session"));
        assert!(!resolution.from_unverified);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_from_request_reply_errors_when_nothing_resolves() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        mount_find(&server, &[]).await;

        let Err(err) = resolve_from(&client, "hi", None, true).await else {
            anyhow::bail!("no confirmed sender and no usable env id must be a tool error");
        };
        assert!(err.message.contains("request_reply"));
        Ok(())
    }
}
