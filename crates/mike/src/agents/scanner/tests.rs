use super::*;

#[test]
fn parses_basic_frontmatter() {
    let content =
        "---\nname: architect\ndescription: Designs features.\nmodel: opus\n---\n\nbody here";
    let p = parse_frontmatter(content);
    assert_eq!(p.name.as_deref(), Some("architect"));
    assert_eq!(p.description.as_deref(), Some("Designs features."));
}

#[test]
fn parses_quoted_values() {
    let content = "---\nname: \"frontend-developer\"\ndescription: 'Builds UIs.'\n---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.name.as_deref(), Some("frontend-developer"));
    assert_eq!(p.description.as_deref(), Some("Builds UIs."));
}

#[test]
fn returns_empty_when_no_frontmatter() {
    let content = "no frontmatter here";
    let p = parse_frontmatter(content);
    assert!(p.name.is_none());
    assert!(p.description.is_none());
}

#[test]
fn handles_crlf_line_endings() {
    let content = "---\r\nname: rusty\r\ndescription: A rust agent.\r\n---\r\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.name.as_deref(), Some("rusty"));
    assert_eq!(p.description.as_deref(), Some("A rust agent."));
}

#[test]
fn handles_bom_prefix() {
    let content = "\u{feff}---\nname: bom-test\n---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.name.as_deref(), Some("bom-test"));
}

#[test]
fn folds_block_scalar_description() {
    let content = "---\n\
        name: review-tests\n\
        description: >-\n  \
          Test coverage and quality reviewer -- detects missing\n  \
          tests, empty assertions, mock abuse, missing edge cases,\n  \
          and test organization problems.\n\
        model: sonnet\n\
        ---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.name.as_deref(), Some("review-tests"));
    assert_eq!(
        p.description.as_deref(),
        Some(
            "Test coverage and quality reviewer -- detects missing \
            tests, empty assertions, mock abuse, missing edge cases, \
            and test organization problems."
        )
    );
}

#[test]
fn folds_block_scalar_with_plain_indicator() {
    let content = "---\n\
        name: x\n\
        description: >\n  \
          line one\n  \
          line two\n\
        ---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.description.as_deref(), Some("line one line two"));
}

#[test]
fn preserves_literal_block_scalar() {
    let content = "---\n\
        name: x\n\
        description: |\n  \
          line one\n  \
          line two\n\
        ---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.description.as_deref(), Some("line one\nline two"));
}

#[test]
fn folds_continuation_after_plain_inline() {
    let content = "---\n\
        name: x\n\
        description: starts here\n  \
          continues here\n\
        ---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.description.as_deref(), Some("starts here continues here"));
}

#[test]
fn empty_description_returns_none() {
    let content = "---\nname: x\ndescription:\nmodel: opus\n---\n";
    let p = parse_frontmatter(content);
    assert!(p.description.is_none());
}

#[test]
fn skills_list_does_not_leak_into_description() {
    let content = "---\n\
        name: x\n\
        description: just this\n\
        skills:\n  \
          - alpha\n  \
          - beta\n\
        ---\n";
    let p = parse_frontmatter(content);
    assert_eq!(p.description.as_deref(), Some("just this"));
}

#[test]
fn parse_body_persona_extracts_first_sentence() {
    let content = "---\nname: rust-developer\n---\n\nYou are a Rust Development Expert -- a senior engineer.\n\nMore body text.";
    assert_eq!(
        parse_body_persona(content).as_deref(),
        Some("Rust Development Expert")
    );
}

#[test]
fn parse_body_persona_returns_none_for_frontmatter_only_file() {
    let content = "---\nname: x\ndescription: y\n---";
    assert_eq!(parse_body_persona(content), None);
}

#[test]
fn parse_body_persona_returns_none_when_no_persona_sentence() {
    let content = "---\nname: x\n---\n\nJust some prose with no persona line.";
    assert_eq!(parse_body_persona(content), None);
}

#[test]
fn dedupe_collapses_duplicate_names_later_wins() {
    let triples = vec![
        (
            "frontend-developer".to_string(),
            "/home/x/.claude/agents/frontend-developer.md".to_string(),
            Some("user-global description".to_string()),
        ),
        (
            "frontend-developer".to_string(),
            "/cwd/.claude/agents/frontend-developer.md".to_string(),
            Some("project-local description".to_string()),
        ),
    ];
    let by_name = dedupe_by_name(triples);
    assert_eq!(by_name.len(), 1);
    let Some((path, description)) = by_name.get("frontend-developer") else {
        panic!("dedupe must keep `frontend-developer` after collision");
    };
    assert_eq!(path, "/cwd/.claude/agents/frontend-developer.md");
    assert_eq!(description.as_deref(), Some("project-local description"));
}

#[test]
fn dedupe_drops_empty_names() {
    let triples = vec![
        (
            String::new(),
            "/path/with/no-name.md".to_string(),
            Some("d".to_string()),
        ),
        ("real".to_string(), "/path/real.md".to_string(), None),
        (
            String::new(),
            "/another/empty.md".to_string(),
            Some("e".to_string()),
        ),
    ];
    let by_name = dedupe_by_name(triples);
    assert_eq!(by_name.len(), 1);
    assert!(by_name.contains_key("real"));
}

#[test]
fn dedupe_preserves_distinct_names() {
    let triples = vec![
        (
            "a".to_string(),
            "/p/a.md".to_string(),
            Some("da".to_string()),
        ),
        ("b".to_string(), "/p/b.md".to_string(), None),
        (
            "c".to_string(),
            "/p/c.md".to_string(),
            Some("dc".to_string()),
        ),
    ];
    let by_name = dedupe_by_name(triples);
    assert_eq!(by_name.len(), 3);
    assert_eq!(
        by_name["a"],
        ("/p/a.md".to_string(), Some("da".to_string()))
    );
    assert_eq!(by_name["b"], ("/p/b.md".to_string(), None));
    assert_eq!(
        by_name["c"],
        ("/p/c.md".to_string(), Some("dc".to_string()))
    );
}

#[test]
fn dedupe_empty_input_returns_empty_map() {
    let triples: Vec<(String, String, Option<String>)> = Vec::new();
    let by_name = dedupe_by_name(triples);
    assert!(by_name.is_empty());
}

#[test]
fn dedupe_yields_sorted_keys() {
    let triples = vec![
        ("zebra".to_string(), "/p/z.md".to_string(), None),
        ("alpha".to_string(), "/p/a.md".to_string(), None),
        ("middle".to_string(), "/p/m.md".to_string(), None),
    ];
    let by_name = dedupe_by_name(triples);
    let keys: Vec<&str> = by_name.keys().map(|s| s.as_str()).collect();
    assert_eq!(keys, vec!["alpha", "middle", "zebra"]);
}
