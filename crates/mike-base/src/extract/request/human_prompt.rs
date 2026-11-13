use serde_json::Value;

use crate::extract::content::extract_text_from_content;

pub fn extract_human_prompt(body: &Value) -> String {
    let last_user = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| {
            msgs.iter()
                .rev()
                .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
        });

    let Some(msg) = last_user else {
        return String::new();
    };

    let content = msg.get("content");
    let Some(content) = content else {
        return String::new();
    };

    if let Some(s) = content.as_str() {
        let t = s.trim_start();
        if t.starts_with('{') || t.starts_with('[') {
            return String::new();
        }
    }

    if let Some(arr) = content.as_array() {
        let has_text = arr
            .iter()
            .any(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"));
        if !has_text {
            return String::new();
        }
    }

    let raw = extract_text_from_content(content);
    let stripped = strip_xml_tag(&raw, "system-reminder");
    let stripped = strip_xml_tag(&stripped, "user-prompt-submit-hook");
    let task_notification_fallback =
        xml_tag_content(&stripped, "task-notification").and_then(task_notification_fallback);
    let stripped = strip_xml_tag(&stripped, "task-notification");

    let has_local_cmd =
        stripped.contains("<local-command-caveat>") || stripped.contains("<local-command-stdout>");
    let command_invocation = xml_tag_content(&stripped, "command-name").map(|name| {
        match xml_tag_content(&stripped, "command-args").filter(|a| !a.trim().is_empty()) {
            Some(args) => format!("{} {}", name.trim(), args.trim()),
            None => name.trim().to_string(),
        }
    });

    if let Some(cmd) = &command_invocation
        && !has_local_cmd
    {
        return cmd.clone();
    }

    let stripped = strip_xml_tag(&stripped, "local-command-caveat");
    let stripped = strip_xml_tag(&stripped, "local-command-stdout");
    let stripped = strip_xml_tag(&stripped, "command-message");
    let stripped = strip_xml_tag(&stripped, "command-args");
    let stripped = strip_xml_tag(&stripped, "command-name");
    let trimmed = stripped.trim();

    if trimmed.is_empty() {
        if let Some(cmd) = command_invocation {
            return cmd;
        }
        if let Some(summary) = task_notification_fallback {
            return summary;
        }
    }
    trimmed.to_string()
}

fn task_notification_fallback(inner: &str) -> Option<String> {
    let summary = xml_tag_content(inner, "summary")?.trim();
    if summary.is_empty() {
        return None;
    }
    Some(format!("Task notification: {summary}"))
}

fn xml_tag_content<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&close)? + start;
    Some(&s[start..end])
}

fn strip_xml_tag(s: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    loop {
        match remaining.find(&open) {
            Some(start) => {
                result.push_str(&remaining[..start]);
                let after_open = &remaining[start + open.len()..];
                match after_open.find(&close) {
                    Some(end) => {
                        remaining = &after_open[end + close.len()..];
                    }
                    None => {
                        result.push_str(&remaining[start..]);
                        break;
                    }
                }
            }
            None => {
                result.push_str(remaining);
                break;
            }
        }
    }
    result
}

#[cfg(test)]
#[path = "human_prompt_tests.rs"]
mod tests;
