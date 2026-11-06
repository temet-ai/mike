use crate::extract::content::extract_text_from_content;

const MARKER: &str = "agentId: ";

pub(crate) fn worker_producer_id(content: &serde_json::Value) -> Option<String> {
    id_from_text(&extract_text_from_content(content))
}

fn id_from_text(text: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for line in text.lines() {
        let Some((_, after_marker)) = line.split_once(MARKER) else {
            continue;
        };
        let token = after_marker.split_whitespace().next().unwrap_or("");
        if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        match found.as_deref() {
            None => found = Some(token.to_string()),
            Some(existing) if existing == token => {}
            Some(_) => return None,
        }
    }
    found
}

#[cfg(test)]
#[path = "agent_result_tests.rs"]
mod tests;
