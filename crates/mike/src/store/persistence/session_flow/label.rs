const MAX_LABEL_BYTES: usize = 128;

pub(super) fn sanitize(value: Option<String>, ordinal: i32) -> Option<String> {
    let cleaned = collapse_whitespace(&value?);
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.len() > MAX_LABEL_BYTES {
        return Some(format!("Agent {ordinal}"));
    }
    Some(cleaned)
}

fn collapse_whitespace(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if ch.is_control() {
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_control_characters() {
        assert_eq!(
            sanitize(Some("Hello\u{0}World".into()), 1),
            Some("HelloWorld".into())
        );
    }

    #[test]
    fn sanitize_collapses_whitespace_runs_and_trims() {
        assert_eq!(
            sanitize(Some("  a   b\t\tc  ".into()), 1),
            Some("a b c".into())
        );
    }

    #[test]
    fn sanitize_keeps_value_at_exactly_the_byte_cap() {
        let value = "a".repeat(MAX_LABEL_BYTES);
        assert_eq!(sanitize(Some(value.clone()), 7), Some(value));
    }

    #[test]
    fn sanitize_falls_back_to_generated_label_one_byte_over_cap() {
        let value = "a".repeat(MAX_LABEL_BYTES + 1);
        assert_eq!(sanitize(Some(value), 7), Some("Agent 7".into()));
    }

    #[test]
    fn sanitize_measures_multibyte_text_in_bytes_not_chars() {
        let value = "é".repeat(MAX_LABEL_BYTES);
        assert_eq!(sanitize(Some(value), 3), Some("Agent 3".into()));
    }

    #[test]
    fn sanitize_none_for_missing_value() {
        assert_eq!(sanitize(None, 1), None);
    }

    #[test]
    fn sanitize_none_for_all_control_characters() {
        assert_eq!(sanitize(Some("\u{0}\u{1}".into()), 1), None);
    }
}
