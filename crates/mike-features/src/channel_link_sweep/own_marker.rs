use serde::Deserialize;

const MARKER: &[u8] = b"claude.ai/code/session_";
const OWN_LINE: &str = "Claude-Session: https://claude.ai/code/session_";
const ATTRIBUTION_OPENING: &str = "Attribution for git commits and pull requests";
const REMINDER_OPEN: &str = "<system-reminder>";
const REMINDER_CLOSE: &str = "</system-reminder>";
const MAX_ID_SCAN_LEN: usize = 128;

#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: Option<Content>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Blocks(Vec<Block>),
    Text(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct Block {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<ToolContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ToolContent {
    Text(String),
    Items(Vec<Block>),
}

pub fn raw_marker_ids(body: &[u8]) -> Vec<&str> {
    let mut ids: Vec<&str> = Vec::new();
    let mut from = 0;
    while let Some(at) = find_marker(&body[from..]) {
        let start = from + at + MARKER.len();
        let len = body[start..]
            .iter()
            .take(MAX_ID_SCAN_LEN)
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        if let Some(id) = body
            .get(start..start + len)
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .filter(|id| !id.is_empty() && !ids.contains(id))
        {
            ids.push(id);
        }
        from = start + len;
    }
    ids
}

fn find_marker(haystack: &[u8]) -> Option<usize> {
    haystack
        .windows(MARKER.len())
        .position(|window| window == MARKER)
}

pub fn own_channel_ids(body: &[u8]) -> Vec<String> {
    let Ok(parsed) = serde_json::from_slice::<Body>(body) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = Vec::new();
    for message in parsed.messages.iter().filter(|m| m.role == "user") {
        let Some(Content::Blocks(blocks)) = &message.content else {
            continue;
        };
        for reminder in blocks.iter().flat_map(own_reminders) {
            for id in own_lines(reminder) {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_owned());
                }
            }
        }
    }
    ids
}

fn own_reminders(block: &Block) -> Vec<&str> {
    let mut found = Vec::new();
    if block.kind == "text"
        && let Some(reminder) = block.text.as_deref().and_then(leading_reminder)
    {
        found.push(reminder);
    }
    if block.kind == "tool_result" {
        collect_closing_reminders(block.content.as_ref(), &mut found);
    }
    found
}

fn collect_closing_reminders<'a>(content: Option<&'a ToolContent>, out: &mut Vec<&'a str>) {
    match content {
        Some(ToolContent::Text(text)) => out.extend(closing_reminder(text)),
        Some(ToolContent::Items(items)) => {
            for item in items {
                out.extend(item.text.as_deref().and_then(closing_reminder));
            }
        }
        None => {}
    }
}

fn leading_reminder(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    let inner = trimmed.strip_prefix(REMINDER_OPEN)?;
    let end = inner.find(REMINDER_CLOSE)?;
    let reminder = &inner[..end];
    reminder
        .trim_start()
        .starts_with(ATTRIBUTION_OPENING)
        .then_some(reminder)
}

fn closing_reminder(text: &str) -> Option<&str> {
    let trimmed = text.trim_end().strip_suffix(REMINDER_CLOSE)?;
    let open = trimmed.rfind(REMINDER_OPEN)?;
    let output = trimmed[..open].strip_suffix("\n\n")?;
    if output.trim().is_empty() {
        return None;
    }
    let reminder = &trimmed[open + REMINDER_OPEN.len()..];
    reminder
        .trim_start()
        .starts_with(ATTRIBUTION_OPENING)
        .then_some(reminder)
}

fn own_lines(reminder: &str) -> impl Iterator<Item = &str> {
    reminder.match_indices(OWN_LINE).filter_map(|(line, _)| {
        let rest = &reminder[line + OWN_LINE.len()..];
        let len = rest
            .bytes()
            .take(MAX_ID_SCAN_LEN)
            .take_while(u8::is_ascii_alphanumeric)
            .count();
        (len > 0).then(|| &rest[..len])
    })
}

#[cfg(test)]
mod tests {
    use super::{own_channel_ids, raw_marker_ids};
    use serde_json::json;

    fn reminder(id: &str) -> String {
        format!(
            "<system-reminder>\nAttribution for git commits and pull requests you create from here on:\n- End git commit messages with:\nClaude-Session: https://claude.ai/code/session_{id}\n- End pull request descriptions with:\nhttps://claude.ai/code/session_{id}\n</system-reminder>"
        )
    }

    fn body(messages: serde_json::Value) -> Vec<u8> {
        json!({"messages": messages}).to_string().into_bytes()
    }

    #[test]
    fn own_channel_ids_reads_both_reminders_of_a_resumed_conversation_in_order() {
        let b = body(json!([
            {"role":"user","content":[{"type":"text","text":reminder("OLD111")}]},
            {"role":"assistant","content":[{"type":"text","text":"ok"}]},
            {"role":"user","content":[{"type":"text","text":reminder("NEW222")}]},
        ]));
        assert_eq!(own_channel_ids(&b), vec!["OLD111", "NEW222"]);
    }

    #[test]
    fn own_channel_ids_ignores_a_marker_inside_a_tool_result() {
        let b = body(json!([
            {"role":"user","content":[{"type":"text","text":reminder("MINE11")}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1",
              "content":"commit abc\n\nClaude-Session: https://claude.ai/code/session_OTHER9"}]},
        ]));
        assert_eq!(own_channel_ids(&b), vec!["MINE11"]);
    }

    #[test]
    fn own_channel_ids_reads_a_reminder_appended_to_a_tool_result_string() {
        let output = format!(
            "ls output\n\ncommit z\n    Claude-Session: https://claude.ai/code/session_TRAILER1\n\n{}",
            reminder("MINE22")
        );
        let b = body(json!([
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":output}]},
        ]));
        assert_eq!(own_channel_ids(&b), vec!["MINE22"]);
    }

    #[test]
    fn own_channel_ids_ignores_an_attribution_text_inside_a_tool_result() {
        let b = body(json!([
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1",
              "content":[{"type":"text","text":reminder("OTHER9")}]}]},
        ]));
        assert!(own_channel_ids(&b).is_empty());
    }

    #[test]
    fn own_channel_ids_ignores_another_sessions_reminder_printed_mid_output() {
        let output = format!("dump:\n{}\nmore output after the dump", reminder("OTHER55"));
        let b = body(json!([
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":output}]},
        ]));
        assert!(own_channel_ids(&b).is_empty());
    }

    #[test]
    fn own_channel_ids_ignores_a_reminder_printed_inside_a_user_text_block_after_other_text() {
        let b = body(json!([
            {"role":"user","content":[{"type":"text","text":format!("look: {}", reminder("OTHER66"))}]},
        ]));
        assert!(own_channel_ids(&b).is_empty());
    }

    #[test]
    fn own_channel_ids_ignores_text_without_the_attribution_opening() {
        let b = body(json!([
            {"role":"user","content":[{"type":"text",
              "text":"Claude-Session: https://claude.ai/code/session_LOOSE1"}]},
        ]));
        assert!(own_channel_ids(&b).is_empty());
    }

    #[test]
    fn own_channel_ids_ignores_assistant_messages_and_unparseable_bodies() {
        let b = body(json!([
            {"role":"assistant","content":[{"type":"text","text":reminder("ASSIST")}]},
        ]));
        assert!(own_channel_ids(&b).is_empty());
        assert!(own_channel_ids(b"not json at all").is_empty());
    }

    #[test]
    fn raw_marker_ids_lists_each_distinct_id_once_in_order() {
        let b = b"x claude.ai/code/session_AAA\" y claude.ai/code/session_BBB z claude.ai/code/session_AAA";
        assert_eq!(raw_marker_ids(b), vec!["AAA", "BBB"]);
    }

    #[test]
    fn raw_marker_ids_marker_with_no_id_or_at_end_yields_nothing() {
        assert!(raw_marker_ids(b"claude.ai/code/session_\"").is_empty());
        assert!(raw_marker_ids(b"tail claude.ai/code/session_").is_empty());
        assert!(raw_marker_ids(b"").is_empty());
    }

    #[test]
    fn raw_marker_ids_caps_a_pathologically_long_run() {
        let mut b = b"claude.ai/code/session_".to_vec();
        b.extend(std::iter::repeat_n(b'a', 500));
        assert_eq!(raw_marker_ids(&b)[0].len(), super::MAX_ID_SCAN_LEN);
    }
}
