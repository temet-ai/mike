use chrono::{DateTime, Utc};
use serde_json::Value;

use super::types::{Arms, Call, Cut, Source, Usage, UsageRow, Who};

pub(super) fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .map(|parsed| parsed.with_timezone(&Utc))
        .unwrap_or_default()
}

pub(super) fn who(
    session: i64,
    agent_instance: Option<i64>,
    agent: Option<&str>,
    origin: &str,
) -> Who {
    Who {
        session_id: session,
        agent_instance_id: agent_instance,
        claude_code_agent_id: agent.map(str::to_string),
        origin: origin.to_string(),
    }
}

pub(super) fn primary(session: i64) -> Who {
    who(session, None, None, "primary")
}

pub(super) fn arms(pairs: &[(i64, &str)]) -> Arms {
    pairs
        .iter()
        .map(|(session, arm)| (*session, arm.to_string()))
        .collect()
}

pub(super) fn cut(who: Who, wire_id: &str, reason: &str, detail: &str, created_at: &str) -> Cut {
    Cut {
        who,
        created_at: at(created_at),
        wire_id: wire_id.to_string(),
        reason: reason.to_string(),
        detail: Some(detail.to_string()),
    }
}

pub(super) fn source(
    who: Who,
    id: i64,
    wire_id: &str,
    file_path: Option<&str>,
    content: Value,
    result_at: &str,
) -> Source {
    Source {
        who,
        id,
        wire_id: wire_id.to_string(),
        file_path: file_path.map(str::to_string),
        input: None,
        content,
        result_at: at(result_at),
    }
}

pub(super) fn call(
    who: Who,
    id: i64,
    tool_name: &str,
    file_path: Option<&str>,
    created_at: &str,
) -> Call {
    Call {
        who,
        id,
        created_at: at(created_at),
        tool_name: tool_name.to_string(),
        file_path: file_path.map(str::to_string),
        input: None,
    }
}

pub(super) fn usage(fresh: i64, complete: bool) -> Usage {
    Usage {
        fresh,
        cache_write: 0,
        cache_read: 0,
        output: 0,
        complete_usage: complete,
    }
}

pub(super) fn usage_row(who: Who, fresh: i64, complete: bool) -> UsageRow {
    UsageRow {
        who,
        fresh,
        cache_write: 0,
        cache_read: 0,
        output: 0,
        complete_usage: Some(complete),
    }
}
