use serde_json::Value;

use super::formatter::{LESSON_REMINDER_OPEN_TAG, SYSTEM_REMINDER_CLOSE_TAG, sanitize_for_inline};

pub(crate) struct LessonMatchCandidate {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) text: String,
}

pub(crate) fn extract_lesson_reminder_blocks(sent_body: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<Value>(sent_body) else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    collect_blocks_from_value(&value, &mut blocks);
    blocks
}

fn collect_blocks_from_value(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => collect_blocks_from_str(s, out),
        Value::Array(items) => {
            for item in items {
                collect_blocks_from_value(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_blocks_from_value(v, out);
            }
        }
        _ => {}
    }
}

fn collect_blocks_from_str(s: &str, out: &mut Vec<String>) {
    let trimmed = s.trim();
    let Some(after_open) = trimmed.strip_prefix(LESSON_REMINDER_OPEN_TAG) else {
        return;
    };
    let Some(close_at) = after_open.find(SYSTEM_REMINDER_CLOSE_TAG) else {
        return;
    };
    out.push(after_open[..close_at].to_string());
}

pub(crate) fn matching_lesson_ids(
    blocks: &[String],
    candidates: &[LessonMatchCandidate],
) -> Vec<i64> {
    candidates
        .iter()
        .filter(|c| {
            let bullet = format!("- [{}] {}\n", sanitize_for_inline(&c.kind), c.text.trim());
            blocks.iter().any(|b| b.contains(&bullet))
        })
        .map(|c| c.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_shape_block() -> String {
        "<system-reminder source=\"proxy-lessons\">\n\
         Lessons recorded from prior sessions:\n\
         - [correction] Always ask before widening scope.\n\
         - [convention] Persistence writes go through PersistHandle methods.\n\
         </system-reminder>"
            .to_string()
    }

    fn wrap_in_sent_body(block: &str) -> Vec<u8> {
        serde_json::json!({
            "model": "claude-x",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "What should I do?"},
                        {"type": "text", "text": block}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    fn wrap_block_quoted_in_prose(block: &str) -> Vec<u8> {
        serde_json::json!({
            "model": "claude-x",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "text",
                            "text": format!(
                                "Here's the block the proxy captured earlier:\n\n{block}\n\nThat's what was injected."
                            )
                        }
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn extract_finds_the_block_inside_a_nested_json_string() {
        let sent_body = wrap_in_sent_body(&real_shape_block());
        let blocks = extract_lesson_reminder_blocks(&sent_body);
        assert_eq!(blocks.len(), 1, "exactly one block must be found");
        assert!(blocks[0].contains("- [correction] Always ask before widening scope.\n"));
        assert!(
            blocks[0]
                .contains("- [convention] Persistence writes go through PersistHandle methods.\n")
        );
    }

    #[test]
    fn extract_rejects_a_block_quoted_in_the_middle_of_other_prose() {
        let sent_body = wrap_block_quoted_in_prose(&real_shape_block());
        let blocks = extract_lesson_reminder_blocks(&sent_body);
        assert!(
            blocks.is_empty(),
            "a quoted/pasted block mid-leaf must not be treated as a genuine injection"
        );
    }

    #[test]
    fn extract_returns_empty_when_body_is_not_json() {
        let blocks = extract_lesson_reminder_blocks(b"not json at all");
        assert!(blocks.is_empty());
    }

    #[test]
    fn extract_returns_empty_when_no_block_present() {
        let sent_body = serde_json::json!({"model": "x", "messages": []})
            .to_string()
            .into_bytes();
        assert!(extract_lesson_reminder_blocks(&sent_body).is_empty());
    }

    #[test]
    fn matching_lesson_ids_matches_on_exact_kind_and_text() {
        let blocks = vec![real_shape_block()];
        let candidates = vec![
            LessonMatchCandidate {
                id: 1,
                kind: "correction".to_string(),
                text: "Always ask before widening scope.".to_string(),
            },
            LessonMatchCandidate {
                id: 2,
                kind: "convention".to_string(),
                text: "Persistence writes go through PersistHandle methods.".to_string(),
            },
            LessonMatchCandidate {
                id: 3,
                kind: "warning".to_string(),
                text: "This lesson was never injected into this request.".to_string(),
            },
        ];
        let matched = matching_lesson_ids(&blocks, &candidates);
        assert_eq!(matched, vec![1, 2], "only the two injected lessons match");
    }

    #[test]
    fn matching_lesson_ids_rejects_a_text_match_with_the_wrong_kind() {
        let blocks = vec![real_shape_block()];
        let candidates = vec![LessonMatchCandidate {
            id: 9,
            kind: "warning".to_string(),
            text: "Always ask before widening scope.".to_string(),
        }];
        assert!(matching_lesson_ids(&blocks, &candidates).is_empty());
    }

    #[test]
    fn matching_lesson_ids_trims_lesson_text_like_the_formatter_does() {
        let blocks = vec![real_shape_block()];
        let candidates = vec![LessonMatchCandidate {
            id: 1,
            kind: "correction".to_string(),
            text: "  Always ask before widening scope.  \n".to_string(),
        }];
        assert_eq!(matching_lesson_ids(&blocks, &candidates), vec![1]);
    }
}
