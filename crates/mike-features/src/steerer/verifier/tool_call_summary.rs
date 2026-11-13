use crate::types::ToolCall;
use serde_json::Value;

pub fn summarise_tool_calls(tool_calls: &[ToolCall]) -> String {
    const MAX_CALLS: usize = 30;
    const MAX_CHARS: usize = 4_000;

    if tool_calls.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(512);
    for (i, tc) in tool_calls.iter().take(MAX_CALLS).enumerate() {
        if out.len() >= MAX_CHARS {
            out.push_str("... (truncated)");
            break;
        }
        let detail = extract_tool_detail(tc);
        let line = if detail.is_empty() {
            format!("{}. {}\n", i + 1, tc.name)
        } else {
            format!("{}. {} — {}\n", i + 1, tc.name, detail)
        };
        if out.len() + line.len() > MAX_CHARS {
            out.push_str("... (truncated)");
            break;
        }
        out.push_str(&line);
    }
    out
}

fn extract_tool_detail(tc: &ToolCall) -> String {
    let Ok(v) = serde_json::from_str::<Value>(&tc.input_json) else {
        return String::new();
    };
    match tc.name.as_str() {
        "Bash" => bash_detail(&v),
        "Read" | "Write" | "Edit" | "MultiEdit" => v
            .get("file_path")
            .and_then(|c| c.as_str())
            .map(|s| s.to_owned())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn bash_detail(input: &Value) -> String {
    let command = input
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| truncate_str(s, 200))
        .unwrap_or_default();
    if input.get("run_in_background").and_then(Value::as_bool) == Some(true) {
        format!("[run_in_background] {command}")
    } else {
        command
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_call(name: &str, input_json: &str) -> ToolCall {
        crate::types::ToolCall {
            tool_id: "test_id".to_string(),
            name: name.to_string(),
            input_json: input_json.to_string(),
            complete: true,
        }
    }

    #[test]
    fn summary_empty_when_no_tool_calls() {
        assert_eq!(summarise_tool_calls(&[]), "");
    }

    #[test]
    fn summary_bash_shows_command() {
        let tc = make_tool_call("Bash", r#"{"command":"cargo nextest run -p mike"}"#);
        let summary = summarise_tool_calls(&[tc]);
        assert!(summary.contains("Bash"));
        assert!(summary.contains("cargo nextest run -p mike"));
    }

    #[test]
    fn summary_bash_in_the_background_says_so() {
        let background = make_tool_call(
            "Bash",
            r#"{"command":"sleep 60 && make test","run_in_background":true}"#,
        );
        let foreground = make_tool_call(
            "Bash",
            r#"{"command":"sleep 60 && make test","run_in_background":false}"#,
        );
        assert!(summarise_tool_calls(&[background]).contains("[run_in_background] sleep 60"));
        assert!(!summarise_tool_calls(&[foreground]).contains("run_in_background"));
    }

    #[test]
    fn summary_edit_shows_file_path() {
        let tc = make_tool_call(
            "Edit",
            r#"{"file_path":"/src/foo.rs","old_string":"a","new_string":"b"}"#,
        );
        let summary = summarise_tool_calls(&[tc]);
        assert!(summary.contains("Edit"));
        assert!(summary.contains("/src/foo.rs"));
    }

    #[test]
    fn summary_unknown_tool_shows_name_only() {
        let tc = make_tool_call("WebFetch", r#"{"url":"https://example.com"}"#);
        let summary = summarise_tool_calls(&[tc]);
        assert!(summary.contains("WebFetch"));
        assert!(!summary.contains("https://example.com"));
    }

    #[test]
    fn summary_multiple_tools_numbered() {
        let calls = vec![
            make_tool_call("Read", r#"{"file_path":"/src/lib.rs"}"#),
            make_tool_call("Bash", r#"{"command":"cargo check"}"#),
        ];
        let summary = summarise_tool_calls(&calls);
        assert!(summary.contains("1."));
        assert!(summary.contains("2."));
        assert!(summary.contains("/src/lib.rs"));
        assert!(summary.contains("cargo check"));
    }
}
