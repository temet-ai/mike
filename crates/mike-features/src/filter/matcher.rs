use super::{FilterKind, FilterRule};

pub fn block_matches(rule: &FilterRule, text: &str) -> bool {
    if !rule.enabled {
        return false;
    }
    match rule.kind {
        FilterKind::TextSubstring => matches_text_substring(text, &rule.value),
        FilterKind::XmlTag => matches_xml_tag(text, &rule.value),
        FilterKind::LazySection | FilterKind::InjectReminder => false,
    }
}

fn matches_text_substring(haystack: &str, needle: &str) -> bool {
    find_text_substring(haystack, needle).is_some()
}

pub fn find_text_substring(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let needle_lower = needle.to_lowercase();
    let mut lower = String::with_capacity(haystack.len());
    let mut byte_map: Vec<usize> = Vec::with_capacity(haystack.len());
    for (orig_idx, ch) in haystack.char_indices() {
        for lc in ch.to_lowercase() {
            for _ in 0..lc.len_utf8() {
                byte_map.push(orig_idx);
            }
            lower.push(lc);
        }
    }
    let start_lower = lower.find(&needle_lower)?;
    let end_lower = start_lower + needle_lower.len();
    let start = byte_map[start_lower];
    let last_orig = byte_map[end_lower - 1];
    let end = last_orig
        + haystack[last_orig..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
    Some((start, end))
}

fn matches_xml_tag(text: &str, tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    let trimmed = text.trim();
    let open_bare = format!("<{tag}>");
    let open_with_attr = format!("<{tag} ");
    let close_tag = format!("</{tag}>");
    let opens = trimmed.starts_with(&open_bare) || trimmed.starts_with(&open_with_attr);
    opens && trimmed.ends_with(&close_tag)
}

#[cfg(test)]
pub mod tests {
    use super::super::{FilterAction, FilterKind, FilterRule, RuleSource};
    use super::block_matches;
    use chrono::Utc;

    fn rule(kind: FilterKind, value: &str) -> FilterRule {
        FilterRule {
            id: 0,
            uuid: uuid::Uuid::new_v4(),
            kind,
            value: value.to_string(),
            enabled: true,
            source: RuleSource::Toml,
            updated_at: Utc::now(),
            sort_order: 0,
            action: FilterAction::Drop,
            text: None,
            trigger_token: None,
            inject_text: None,
            project_path: None,
            style_override: false,
            title: None,
        }
    }

    #[test]
    fn text_substring_is_case_insensitive() {
        let r = rule(FilterKind::TextSubstring, "System-Reminder");
        assert!(block_matches(&r, "blah system-reminder blah"));
        assert!(block_matches(&r, "<SYSTEM-REMINDER>x</SYSTEM-REMINDER>"));
    }

    #[test]
    fn text_substring_misses_when_not_present() {
        let r = rule(FilterKind::TextSubstring, "kittens");
        assert!(!block_matches(&r, "the quick brown fox"));
    }

    #[test]
    fn xml_tag_matches_bare_open() {
        let r = rule(FilterKind::XmlTag, "system-reminder");
        assert!(block_matches(
            &r,
            "<system-reminder>SessionStart hook success: OK</system-reminder>"
        ));
    }

    #[test]
    fn xml_tag_matches_with_leading_and_trailing_whitespace() {
        let r = rule(FilterKind::XmlTag, "system-reminder");
        assert!(block_matches(
            &r,
            "  \n<system-reminder>hi</system-reminder>\n  "
        ));
    }

    #[test]
    fn xml_tag_matches_with_attributes_on_open() {
        let r = rule(FilterKind::XmlTag, "system-reminder");
        assert!(block_matches(
            &r,
            r#"<system-reminder id="hooks">x</system-reminder>"#
        ));
    }

    #[test]
    fn xml_tag_requires_whole_block_wrap() {
        let r = rule(FilterKind::XmlTag, "system-reminder");
        assert!(!block_matches(
            &r,
            "prefix <system-reminder>x</system-reminder> suffix"
        ));
    }

    #[test]
    fn xml_tag_rejects_mismatched_close() {
        let r = rule(FilterKind::XmlTag, "system-reminder");
        assert!(!block_matches(&r, "<system-reminder>hello"));
    }

    #[test]
    fn disabled_rule_never_matches() {
        let mut r = rule(FilterKind::TextSubstring, "anything");
        r.enabled = false;
        assert!(!block_matches(&r, "contains anything right here"));
    }

    #[test]
    fn empty_value_never_matches() {
        let r = rule(FilterKind::TextSubstring, "");
        assert!(!block_matches(&r, "anything goes"));
        let r2 = rule(FilterKind::XmlTag, "");
        assert!(!block_matches(&r2, "<>x</>"));
    }

    use super::find_text_substring;

    #[test]
    fn find_ascii_returns_original_offsets() {
        assert_eq!(find_text_substring("Keep THIS bit", "this"), Some((5, 9)));
    }

    #[test]
    fn find_first_occurrence_wins() {
        assert_eq!(find_text_substring("abc abc", "ABC"), Some((0, 3)));
    }

    #[test]
    fn find_empty_needle_returns_none() {
        assert_eq!(find_text_substring("anything", ""), None);
    }

    #[test]
    fn find_absent_needle_returns_none() {
        assert_eq!(find_text_substring("haystack", "missing"), None);
    }

    #[test]
    fn find_non_ascii_shrinking_fold_maps_back_to_original_bytes() -> anyhow::Result<()> {
        let hay = "via STRAẞE end";
        let (start, end) = find_text_substring(hay, "straße")
            .ok_or_else(|| anyhow::anyhow!("expected a match"))?;
        assert_eq!(&hay[start..end], "STRAẞE");
        Ok(())
    }

    #[test]
    fn find_expanding_fold_widens_to_whole_original_char() -> anyhow::Result<()> {
        let hay = "İstanbul";
        let (start, end) =
            find_text_substring(hay, "i").ok_or_else(|| anyhow::anyhow!("expected a match"))?;
        assert_eq!(start, 0);
        assert_eq!(&hay[start..end], "İ");
        Ok(())
    }

    #[test]
    fn find_full_text_match_spans_whole_haystack() {
        let hay = "Exact Block Text";
        assert_eq!(
            find_text_substring(hay, "exact block text"),
            Some((0, hay.len()))
        );
    }

    #[derive(serde::Deserialize)]
    struct MatcherVector {
        #[serde(rename = "id")]
        _id: String,
        kind: FilterKind,
        value: String,
        block: String,
        expected_match: bool,
        #[serde(default, rename = "note")]
        _note: Option<String>,
    }

    const VECTORS_JSON: &str = include_str!("../../tests/fixtures/matcher/vectors.json");

    #[test]
    fn vectors_match_shared_fixture() -> anyhow::Result<()> {
        let vectors: Vec<MatcherVector> = serde_json::from_str(VECTORS_JSON)?;
        assert!(
            !vectors.is_empty(),
            "matcher vectors.json must contain at least one vector"
        );
        for v in vectors {
            let r = rule(v.kind, &v.value);
            let got = block_matches(&r, &v.block);
            assert_eq!(
                got, v.expected_match,
                "vector id={:?} kind={:?} value={:?} block={:?}: \
                 expected block_matches = {}, got {}",
                v._id, v.kind, v.value, v.block, v.expected_match, got
            );
        }
        Ok(())
    }
}
