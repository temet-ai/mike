pub fn scan_recall_tags(text: &str) -> Vec<String> {
    const PREFIX: &str = "<mem:recall topic=\"";
    const SUFFIX: &str = "\"/>";

    let mut topics: Vec<String> = Vec::new();
    let mut search_from = 0usize;

    while let Some(start) = text[search_from..].find(PREFIX) {
        let abs_start = search_from + start + PREFIX.len();
        if abs_start > text.len() {
            break;
        }
        let rest = &text[abs_start..];
        if let Some(end) = rest.find(SUFFIX) {
            let topic = rest[..end].trim().to_string();
            if !topic.is_empty() && !topics.contains(&topic) {
                topics.push(topic);
            }
            search_from = abs_start + end + SUFFIX.len();
        } else {
            break;
        }
    }

    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tag_extracted() {
        let text = r#"Some text <mem:recall topic="db-ports"/> more text"#;
        assert_eq!(scan_recall_tags(text), vec!["db-ports"]);
    }

    #[test]
    fn multiple_distinct_tags() {
        let text = r#"<mem:recall topic="alpha"/> and <mem:recall topic="beta"/>"#;
        assert_eq!(scan_recall_tags(text), vec!["alpha", "beta"]);
    }

    #[test]
    fn duplicate_topics_deduplicated() {
        let text = r#"<mem:recall topic="x"/> then <mem:recall topic="x"/>"#;
        assert_eq!(scan_recall_tags(text), vec!["x"]);
    }

    #[test]
    fn empty_topic_skipped() {
        let text = r#"<mem:recall topic=""/> other"#;
        assert!(scan_recall_tags(text).is_empty());
    }

    #[test]
    fn no_tags_returns_empty() {
        assert!(scan_recall_tags("hello world").is_empty());
    }

    #[test]
    fn partial_tag_skipped() {
        let text = r#"<mem:recall topic="half"#;
        assert!(scan_recall_tags(text).is_empty());
    }

    #[test]
    fn whitespace_trimmed_from_topic() {
        let text = r#"<mem:recall topic="  spaced  "/>"#;
        assert_eq!(scan_recall_tags(text), vec!["spaced"]);
    }
}
