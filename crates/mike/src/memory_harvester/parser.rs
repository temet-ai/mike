#[derive(Debug, PartialEq)]
pub struct UpdateTag {
    pub topic: String,
    pub kind: String,
    pub text: String,
}

pub fn scan_update_tags(text: &str) -> Vec<UpdateTag> {
    let mut result = Vec::new();
    let mut search_from = 0;

    while let Some(open_pos) = text[search_from..].find("<mem:update ") {
        let abs_open = search_from + open_pos;

        let Some(tag_end_rel) = text[abs_open..].find('>') else {
            break;
        };
        let tag_end = abs_open + tag_end_rel;
        let open_tag = &text[abs_open..=tag_end];

        search_from = tag_end + 1;

        let Some(topic) = extract_attr(open_tag, "topic") else {
            continue;
        };
        if topic.is_empty() {
            continue;
        }

        let kind = normalise_kind(extract_attr(open_tag, "kind"));

        let close_tag = "</mem:update>";
        let Some(close_rel) = text[search_from..].find(close_tag) else {
            continue;
        };
        let close_pos = search_from + close_rel;

        let inner = text[tag_end + 1..close_pos].trim();
        if inner.is_empty() {
            search_from = close_pos + close_tag.len();
            continue;
        }

        result.push(UpdateTag {
            topic,
            kind,
            text: inner.to_string(),
        });

        search_from = close_pos + close_tag.len();
    }

    result
}

const LESSON_KINDS: [&str; 4] = ["correction", "convention", "warning", "preference"];
const DEFAULT_KIND: &str = "convention";

fn normalise_kind(raw: Option<String>) -> String {
    let lowered = raw.unwrap_or_default().to_lowercase();
    LESSON_KINDS
        .iter()
        .find(|known| **known == lowered)
        .copied()
        .unwrap_or(DEFAULT_KIND)
        .to_string()
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(needle.as_str())? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tag_with_default_kind() {
        let text = r#"Some text. <mem:update topic="db-ports">Use :6666 not :5440.</mem:update> More text."#;
        let tags = scan_update_tags(text);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].topic, "db-ports");
        assert_eq!(tags[0].kind, "convention");
        assert_eq!(tags[0].text, "Use :6666 not :5440.");
    }

    #[test]
    fn parses_explicit_kind_attribute() {
        let text = r#"<mem:update topic="proxy-test" kind="warning">Don't use the shared DB.</mem:update>"#;
        let tags = scan_update_tags(text);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].kind, "warning");
    }

    #[test]
    fn each_known_kind_is_kept() {
        for kind in LESSON_KINDS {
            let text = format!(r#"<mem:update topic="t" kind="{kind}">Body.</mem:update>"#);
            let tags = scan_update_tags(&text);
            assert_eq!(tags.len(), 1);
            assert_eq!(tags[0].kind, kind);
        }
    }

    #[test]
    fn kind_is_matched_ignoring_case() {
        let text = r#"<mem:update topic="t" kind="Warning">Body.</mem:update>"#;
        assert_eq!(scan_update_tags(text)[0].kind, "warning");
    }

    #[test]
    fn unknown_kind_falls_back_to_convention() {
        let text = r#"<mem:update topic="t" kind="hint">Body.</mem:update>"#;
        assert_eq!(scan_update_tags(text)[0].kind, "convention");
    }

    #[test]
    fn option_list_copied_verbatim_falls_back_to_convention() {
        let text = r#"<mem:update topic="t" kind="correction|convention|warning|preference">Body.</mem:update>"#;
        assert_eq!(scan_update_tags(text)[0].kind, "convention");
    }

    #[test]
    fn parses_multiple_tags() {
        let text = r#"<mem:update topic="a">First.</mem:update> gap <mem:update topic="b">Second.</mem:update>"#;
        let tags = scan_update_tags(text);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].topic, "a");
        assert_eq!(tags[1].topic, "b");
    }

    #[test]
    fn skips_tag_missing_topic() {
        let text = r#"<mem:update kind="warning">No topic here.</mem:update>"#;
        let tags = scan_update_tags(text);
        assert!(tags.is_empty(), "tag without topic must be skipped");
    }

    #[test]
    fn skips_empty_topic() {
        let text = r#"<mem:update topic="">Empty topic.</mem:update>"#;
        let tags = scan_update_tags(text);
        assert!(tags.is_empty(), "tag with empty topic must be skipped");
    }

    #[test]
    fn skips_empty_body() {
        let text = r#"<mem:update topic="x"></mem:update>"#;
        let tags = scan_update_tags(text);
        assert!(tags.is_empty(), "tag with empty body must be skipped");
    }

    #[test]
    fn skips_unclosed_tag() {
        let text = r#"<mem:update topic="x">No closing tag."#;
        let tags = scan_update_tags(text);
        assert!(tags.is_empty(), "unclosed tag must be skipped");
    }

    #[test]
    fn trims_inner_whitespace() {
        let text = "<mem:update topic=\"y\">  trimmed content  </mem:update>";
        let tags = scan_update_tags(text);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].text, "trimmed content");
    }

    #[test]
    fn empty_text_returns_empty_vec() {
        assert!(scan_update_tags("").is_empty());
    }

    #[test]
    fn no_tags_returns_empty_vec() {
        assert!(scan_update_tags("just regular text").is_empty());
    }
}
