use std::path::PathBuf;

use regex::Regex;

fn resolve_rules_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MIKE_STYLE_RULES_PATH") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".claude/rules/response-style.md"))
}

pub fn load_banned_terms() -> Option<Vec<String>> {
    load_banned_terms_at(&resolve_rules_path()?)
}

fn load_banned_terms_at(path: &std::path::Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(path).ok()?;
    Some(banned_terms_from_content(&content))
}

pub fn banned_terms_from_content(content: &str) -> Vec<String> {
    let mut terms = Vec::new();

    if let Some(sentence) =
        extract_sentence_after(content, "are banned in anything you write for me:")
    {
        for item in sentence.split(',') {
            let stripped = item.split('(').next().unwrap_or(item);
            let cleaned = stripped.trim().trim_end_matches('.').trim();
            if !cleaned.is_empty() {
                terms.push(cleaned.to_string());
            }
        }
    }

    for marker in [
        "That includes",
        "Do not open a sentence or a paragraph with",
    ] {
        if let Some(sentence) = extract_sentence_after(content, marker) {
            for phrase in quoted_phrases(sentence) {
                let trimmed = phrase.trim();
                if !trimmed.is_empty() {
                    terms.push(trimmed.to_string());
                }
            }
        }
    }

    terms
}

fn extract_sentence_after<'a>(content: &'a str, marker: &str) -> Option<&'a str> {
    let marker_pattern = marker
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+");
    let marker_re = Regex::new(&marker_pattern).ok()?;
    let start = marker_re.find(content)?.end();
    let rest = &content[start..];
    let mut paren_depth: i32 = 0;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '.' if paren_depth <= 0 => {
                let after = rest[idx + 1..].trim_start();
                let ends_sentence = match after.chars().next() {
                    None => true,
                    Some(c) => c.is_uppercase(),
                };
                if ends_sentence {
                    return Some(&rest[..idx]);
                }
            }
            _ => {}
        }
    }
    None
}

fn quoted_phrases(s: &str) -> Vec<String> {
    s.split('"')
        .skip(1)
        .step_by(2)
        .map(ToString::to_string)
        .collect()
}

pub fn find_banned_term(text: &str, banned: &[String]) -> Option<String> {
    banned.iter().find_map(|term| {
        if term.is_empty() {
            return None;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(term));
        let re = Regex::new(&pattern).ok()?;
        re.is_match(text).then(|| term.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES_FIXTURE: &str = r#"Do not invent terms or reuse the model's own house jargon. The following words and
their relatives are banned in anything you write for me: load-bearing, prose (say text
or writing), hand-waving, reflexive, honest framing, unlock (as a noun), oracle,
constellation, gate, ledger, erratum, premise, idiom, landed, surface (as a verb),
nuance, orthogonal, crisp, tight, clean (as a verdict), sharp, robust. If a plain word
exists, use the plain word. If you are choosing the most specific word you know instead
of the one an ordinary colleague would use, you have picked the wrong word.

Sentences and labels that exist only to announce the next sentence are banned. That
includes "one thing", "one thing does not match", "honest caveat", "worth noting",
"needs your decision", "one side effect", "the catch is", "note that", and any other
phrase that tells me a point is important instead of making the point. Counting the
items is the same move under a different name. Do not open a sentence or a paragraph
with "One thing", "Two things", "Three things", "Four things", "Two separate problems",
"Two smaller things", "Where things stand", "Both confirmed", or any other count or
label that stands in for the items themselves."#;

    #[test]
    fn banned_terms_from_content_extracts_the_comma_separated_list() {
        let terms = banned_terms_from_content(RULES_FIXTURE);
        for expected in [
            "load-bearing",
            "prose",
            "hand-waving",
            "reflexive",
            "honest framing",
            "unlock",
            "oracle",
            "constellation",
            "gate",
            "ledger",
            "erratum",
            "premise",
            "idiom",
            "landed",
            "surface",
            "nuance",
            "orthogonal",
            "crisp",
            "tight",
            "clean",
            "sharp",
            "robust",
        ] {
            assert!(
                terms.iter().any(|t| t == expected),
                "expected {expected:?} in extracted terms: {terms:?}"
            );
        }
    }

    #[test]
    fn banned_terms_from_content_strips_trailing_parentheticals() {
        let terms = banned_terms_from_content(RULES_FIXTURE);
        assert!(
            !terms.iter().any(|t| t.contains('(')),
            "no extracted term should retain its parenthetical annotation: {terms:?}"
        );
        assert!(!terms.iter().any(|t| t == "say text\nor writing)"));
    }

    #[test]
    fn banned_terms_from_content_extracts_quoted_announcing_phrases() {
        let terms = banned_terms_from_content(RULES_FIXTURE);
        for expected in [
            "one thing",
            "one thing does not match",
            "honest caveat",
            "worth noting",
            "needs your decision",
            "one side effect",
            "the catch is",
            "note that",
        ] {
            assert!(
                terms.iter().any(|t| t == expected),
                "expected quoted phrase {expected:?} in extracted terms: {terms:?}"
            );
        }
    }

    #[test]
    fn banned_terms_from_content_extracts_quoted_counting_labels() {
        let terms = banned_terms_from_content(RULES_FIXTURE);
        for expected in [
            "One thing",
            "Two things",
            "Three things",
            "Four things",
            "Two separate problems",
            "Two smaller things",
            "Where things stand",
            "Both confirmed",
        ] {
            assert!(
                terms.iter().any(|t| t == expected),
                "expected counting label {expected:?} in extracted terms: {terms:?}"
            );
        }
    }

    #[test]
    fn banned_terms_from_content_returns_empty_when_markers_absent() {
        assert!(banned_terms_from_content("nothing relevant here").is_empty());
    }

    #[test]
    fn find_banned_term_flags_rule_71_before_load_bearing() {
        let banned = banned_terms_from_content(RULES_FIXTURE);
        let before = "Before your first tool call, say in a sentence what you're about to do; \
                       while working, give brief updates when you find something load-bearing \
                       or change direction.";
        let hit = find_banned_term(before, &banned);
        assert_eq!(hit.as_deref(), Some("load-bearing"), "got: {hit:?}");
    }

    #[test]
    fn find_banned_term_flags_rule_71_before_surface_as_verb() {
        let banned = banned_terms_from_content(RULES_FIXTURE);
        let before = "Before deleting or overwriting, look at the target. If what you find \
                       contradicts how it was described, or you didn't create it, surface \
                       that instead of proceeding.";
        let hit = find_banned_term(before, &banned);
        assert_eq!(hit.as_deref(), Some("surface"), "got: {hit:?}");
    }

    #[test]
    fn find_banned_term_passes_rule_71_after() {
        let banned = banned_terms_from_content(RULES_FIXTURE);
        let after = "Before your first tool call, say in a sentence what you're about to do; \
                      while working, give brief updates when you find something that changes \
                      what you are doing, or when you change direction.";
        assert_eq!(find_banned_term(after, &banned), None);
    }

    #[test]
    fn find_banned_term_flags_rule_68_before_gate() {
        let banned = banned_terms_from_content(RULES_FIXTURE);
        let before = "Nothing else ends a turn — not a discovery that widens the work, not a \
                       flaky tool, not a red gate you did not cause. Before ending, name any \
                       acceptance criterion you did not exercise.";
        let hit = find_banned_term(before, &banned);
        assert_eq!(hit.as_deref(), Some("gate"), "got: {hit:?}");
    }

    #[test]
    fn find_banned_term_passes_rule_68_after() {
        let banned = banned_terms_from_content(RULES_FIXTURE);
        let after = "Nothing else ends a turn — not a discovery that widens the work, not a \
                      flaky tool, not a red check you did not cause. Before ending, say \
                      plainly anything you were asked for and did not do.";
        assert_eq!(find_banned_term(after, &banned), None);
    }

    #[test]
    fn find_banned_term_is_case_insensitive() {
        let banned = vec!["gate".to_string()];
        assert_eq!(
            find_banned_term("this is GATEd behavior discussion: Gate check", &banned),
            Some("gate".to_string())
        );
    }

    #[test]
    fn find_banned_term_requires_whole_word_boundary() {
        let banned = vec!["gate".to_string()];
        assert_eq!(
            find_banned_term("the delegate walked to the gateway", &banned),
            None
        );
    }

    #[test]
    fn find_banned_term_matches_multi_word_phrase() {
        let banned = vec!["honest framing".to_string()];
        assert_eq!(
            find_banned_term("that is an honest framing of the tradeoff", &banned),
            Some("honest framing".to_string())
        );
        assert_eq!(
            find_banned_term("that framing was honest but incomplete", &banned),
            None
        );
    }

    #[test]
    fn find_banned_term_returns_none_for_empty_list() {
        assert_eq!(find_banned_term("anything at all", &[]), None);
    }

    #[test]
    fn load_banned_terms_none_when_path_is_a_missing_file() {
        let result =
            load_banned_terms_at(std::path::Path::new("/nonexistent/path/response-style.md"));
        assert!(
            result.is_none(),
            "a missing rules file must yield None (validation passes), not a panic or Some(empty)"
        );
    }

    #[test]
    fn load_banned_terms_reads_the_given_path() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("response-style.md");
        std::fs::write(&path, RULES_FIXTURE)?;
        let result = load_banned_terms_at(&path);
        let terms = result.ok_or_else(|| anyhow::anyhow!("expected Some(terms), got None"))?;
        assert!(terms.iter().any(|t| t == "gate"));
        Ok(())
    }
}
