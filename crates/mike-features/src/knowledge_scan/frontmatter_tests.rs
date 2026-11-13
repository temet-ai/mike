use super::frontmatter::parse;
use serde_json::json;

#[test]
fn parse_reads_flat_and_nested_memory_frontmatter() {
    let text = "---\nname: feedback-worktrees\ndescription: Worktrees live inside the repo\nmetadata:\n  type: feedback\n---\n\nBody\n";
    let map = parse(text);
    assert_eq!(map["name"], json!("feedback-worktrees"));
    assert_eq!(map["description"], json!("Worktrees live inside the repo"));
    assert_eq!(map["metadata"], json!({"type": "feedback"}));
}

#[test]
fn parse_joins_folded_block_scalars_and_quoted_values() {
    let text =
        "---\nname: \"quoted-name\"\ndescription: >-\n  first line\n  second line\n---\nbody";
    let map = parse(text);
    assert_eq!(map["name"], json!("quoted-name"));
    assert_eq!(map["description"], json!("first line second line"));
}

#[test]
fn parse_reads_lists_of_scalars() {
    let map = parse("---\ntags:\n  - a\n  - \"b\"\n---\n");
    assert_eq!(map["tags"], json!(["a", "b"]));
}

#[test]
fn parse_without_frontmatter_is_empty() {
    assert!(parse("# Title\n\ntext").is_empty());
}

#[test]
fn parse_unclosed_frontmatter_reports_the_reason() {
    let map = parse("---\nname: x\nno end");
    assert_eq!(map["frontmatter_error"], json!("unclosed frontmatter"));
}

#[test]
fn parse_line_without_a_key_reports_the_line_number() {
    let map = parse("---\nname: x\njust words\n---\n");
    let message = map["frontmatter_error"].as_str().unwrap_or_default();
    assert!(message.contains("line 2"), "{message}");
}

#[test]
fn parse_indentation_before_any_key_is_an_error() {
    let map = parse("---\n  stray: 1\n---\n");
    assert!(map.contains_key("frontmatter_error"));
}
