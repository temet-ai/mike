use serde_json::Value;

pub(crate) struct ProjectSignals<'a> {
    pub system_prompt: Option<&'a str>,
    pub message_sources: Vec<&'a str>,
}

pub(crate) fn collect_project_scan_sources(body: &Value) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) else {
        return out;
    };
    for msg in msgs {
        let Some(content) = msg.get("content") else {
            continue;
        };
        append_content_text_slices(content, &mut out);
    }
    out
}

fn append_content_text_slices<'a>(content: &'a Value, out: &mut Vec<&'a str>) {
    match content {
        Value::String(s) => out.push(s.as_str()),
        Value::Array(blocks) => {
            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            out.push(t);
                        }
                    }
                    Some("tool_result") => {
                        if let Some(nested) = block.get("content") {
                            append_content_text_slices(nested, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

pub(super) fn find_working_directory_path<'a>(sources: &[&'a str]) -> Option<&'a str> {
    for source in sources {
        for line in source.lines() {
            let stripped = line.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '*');
            let rest = if let Some(r) = stripped.strip_prefix("Primary working directory:") {
                r
            } else if let Some(r) = stripped.strip_prefix("Working directory:") {
                r
            } else {
                continue;
            };
            let path = rest.trim();
            if !path.starts_with('/') {
                continue;
            }
            return Some(path.trim_end_matches('/'));
        }
    }
    None
}

pub(super) fn trusted_working_directory_sources<'a>(
    system_prompt: Option<&'a str>,
    message_sources: &[&'a str],
) -> Vec<&'a str> {
    let mut out = Vec::new();
    if let Some(sp) = system_prompt {
        out.push(sp);
    }
    for source in message_sources {
        out.extend(system_reminder_blocks(source));
    }
    out
}

fn system_reminder_blocks(s: &str) -> Vec<&str> {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = Vec::new();
    let mut remaining = s;
    while let Some(start) = remaining.find(OPEN) {
        let after_open = &remaining[start + OPEN.len()..];
        match after_open.find(CLOSE) {
            Some(end) => {
                out.push(&after_open[..end]);
                remaining = &after_open[end + CLOSE.len()..];
            }
            None => break,
        }
    }
    out
}

pub(super) fn has_unsafe_path_chars(root: &str) -> bool {
    root.chars().any(|c| {
        c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '\\' | '\'' | '"' | '`' | '{' | '}' | '$' | '*' | '?' | '<' | '>' | '|'
            )
    })
}

#[cfg(test)]
#[path = "project_scan_tests.rs"]
mod tests;
