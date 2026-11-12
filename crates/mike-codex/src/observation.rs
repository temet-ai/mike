use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Active,
    Idle,
    Ended,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadUsage {
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionUpdate {
    pub thread_id: String,
    pub lifecycle: Option<SessionPhase>,
    pub observed_model: Option<String>,
    pub usage: Option<ThreadUsage>,
    pub item: Option<ItemUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemUpdate {
    pub item_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationObservation {
    Session(SessionUpdate),
    Gap { dropped: u64 },
}

pub fn observe_notification(event: &Value) -> Option<NotificationObservation> {
    let method = event.get("method")?.as_str()?;
    let params = event.get("params")?;
    if method == "mike/observationGap" {
        return Some(NotificationObservation::Gap {
            dropped: params.get("dropped").and_then(Value::as_u64).unwrap_or(1),
        });
    }
    let thread = params
        .get("threadId")
        .or_else(|| params.pointer("/thread/id"))?
        .as_str()?;
    let lifecycle = match method {
        "thread/started" => Some(SessionPhase::Idle),
        "turn/started" => Some(SessionPhase::Active),
        "turn/completed" => Some(SessionPhase::Idle),
        "thread/closed" => Some(SessionPhase::Ended),
        "thread/status/changed" => phase(params.pointer("/status/type").and_then(Value::as_str)),
        "item/started" | "item/completed" | "thread/tokenUsage/updated" => None,
        _ => return None,
    };
    Some(NotificationObservation::Session(SessionUpdate {
        thread_id: thread.to_owned(),
        lifecycle,
        observed_model: params
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        usage: (method == "thread/tokenUsage/updated")
            .then(|| usage(params))
            .flatten(),
        item: item(method, params),
    }))
}

fn item(method: &str, params: &Value) -> Option<ItemUpdate> {
    let (value, is_turn) = match method {
        "turn/started" | "turn/completed" => (params.get("turn")?, true),
        "item/started" | "item/completed" => (params.get("item")?, false),
        _ => return None,
    };
    let id = value.get("id")?.as_str()?;
    let kind = if is_turn {
        "turn"
    } else {
        value.get("type")?.as_str()?
    };
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if method.ends_with("started") {
                "in_progress"
            } else {
                "completed"
            }
        });
    Some(ItemUpdate {
        item_id: if is_turn {
            format!("turn:{id}")
        } else {
            id.to_owned()
        },
        turn_id: if is_turn {
            Some(id.to_owned())
        } else {
            params
                .get("turnId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        },
        kind: kind.to_owned(),
        name: value.get("tool").and_then(Value::as_str).map(str::to_owned),
        status: Some(status.to_owned()),
        success: match status {
            "completed" => Some(true),
            "failed" => Some(false),
            _ => None,
        },
        exit_code: value
            .get("exitCode")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok()),
    })
}

fn phase(status: Option<&str>) -> Option<SessionPhase> {
    match status {
        Some("idle") => Some(SessionPhase::Idle),
        Some("active") => Some(SessionPhase::Active),
        Some("systemError") => Some(SessionPhase::Failed),
        Some("notLoaded") => Some(SessionPhase::Ended),
        _ => None,
    }
}

fn usage(params: &Value) -> Option<ThreadUsage> {
    let usage = params.get("tokenUsage")?;
    let total = usage.get("total")?.as_object()?;
    let count = |name| total.get(name).and_then(Value::as_i64).filter(|n| *n >= 0);
    Some(ThreadUsage {
        input_tokens: count("inputTokens"),
        cached_input_tokens: count("cachedInputTokens"),
        output_tokens: count("outputTokens"),
        reasoning_output_tokens: count("reasoningOutputTokens"),
        total_tokens: count("totalTokens"),
        context_window: usage
            .get("modelContextWindow")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_usage_preserves_unknown_totals_and_ignores_last_call() -> anyhow::Result<()> {
        let event = json!({"method":"thread/tokenUsage/updated","params":{
            "threadId":"thread","tokenUsage":{"total":{"inputTokens":9},"last":{"totalTokens":300}}
        }});
        let Some(NotificationObservation::Session(update)) = observe_notification(&event) else {
            anyhow::bail!("missing update");
        };
        let usage = update
            .usage
            .ok_or_else(|| anyhow::anyhow!("missing usage"))?;
        assert_eq!(usage.input_tokens, Some(9));
        assert_eq!(usage.total_tokens, None);
        Ok(())
    }

    #[test]
    fn turn_state_is_metadata_only_and_unknown_methods_are_ignored() -> anyhow::Result<()> {
        let event = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{
            "id":"turn","status":"failed","items":[{"text":"private response"}]
        }}});
        let Some(NotificationObservation::Session(update)) = observe_notification(&event) else {
            anyhow::bail!("missing update");
        };
        assert_eq!(update.lifecycle, Some(SessionPhase::Idle));
        assert_eq!(
            update.item.and_then(|i| i.status).as_deref(),
            Some("failed")
        );
        assert!(observe_notification(&json!({"method":"unknown","params":{}})).is_none());
        Ok(())
    }
}
