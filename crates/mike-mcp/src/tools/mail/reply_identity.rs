use std::time::Duration;

use rmcp::ErrorData as RmcpError;
use serde_json::Value;

use crate::caller::CallerSession;
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
    caller_session: Option<CallerSession<'_>>,
) -> Result<ReplyResolution, RmcpError> {
    if let Some(CallerSession::Codex(session)) = caller_session
        && is_session_uuid(session)
    {
        if let Some(from) = explicit_from
            && !from.eq_ignore_ascii_case(session)
        {
            return Err(refusal_error(from, session));
        }
        return Ok(ReplyResolution {
            from: Some(session.to_string()),
            from_unverified: false,
        });
    }
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

    if let Some(CallerSession::Process(session)) = caller_session
        && is_session_uuid(session)
        && session_has_main_class_traffic(client, session).await
    {
        return Ok(ReplyResolution {
            from: Some(session.to_string()),
            from_unverified: true,
        });
    }

    Err(RmcpError::invalid_params(
        "Not sent: this session's identity could not be resolved, and mail with no sender \
         reaches the recipient as from an unknown sender. No trusted caller session was \
         supplied and no single session's captured send_session_message tool_use carries \
         this exact message. Check Mike's Codex observation or the process session \
         configuration; pass `from` only with your own Mike session UUID.",
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
             {resolved}. Resend with from={resolved}, or omit \
             `from`."
        ),
        None,
    )
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

async fn session_has_main_class_traffic(client: &InspectClient, session_id: &str) -> bool {
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
#[path = "reply_identity_tests.rs"]
mod tests;
