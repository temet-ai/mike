use super::rule_test_support::new_drop_rule;
use super::{
    FilterConfig, FilterKind, FilterOutcome, apply_request_filter, canonicalize_project_path,
    project_scope_matches,
};

#[test]
fn canonicalize_absolute_path_yields_deriver_form() {
    assert_eq!(
        canonicalize_project_path("/Users/vimes/projects/temet/temet-proxy"),
        "/Users/vimes/projects/temet/temet-proxy"
    );
}

#[test]
fn canonicalize_trims_trailing_slash_and_whitespace() {
    assert_eq!(
        canonicalize_project_path("  /Users/vimes/projects/temet/temet-proxy/ "),
        "/Users/vimes/projects/temet/temet-proxy"
    );
}

#[test]
fn canonicalize_three_segment_path_keeps_last_segment_only() {
    assert_eq!(
        canonicalize_project_path("/test/dir/project"),
        "/test/dir/project"
    );
}

#[test]
fn canonicalize_short_path_keeps_all_segments() {
    assert_eq!(canonicalize_project_path("/a/b"), "/a/b");
}

#[test]
fn canonicalize_already_canonical_is_passthrough() {
    assert_eq!(
        canonicalize_project_path("projects/temet/temet-proxy"),
        "projects/temet/temet-proxy"
    );
}

#[test]
fn scope_matches_exact_canonical() {
    assert!(project_scope_matches(
        "projects/temet/temet-proxy",
        "projects/temet/temet-proxy"
    ));
}

#[test]
fn scope_matches_legacy_trimmed_rule_path() {
    assert!(project_scope_matches(
        "projects/temet/temet-proxy",
        "/Users/vimes/projects/temet/temet-proxy"
    ));
}

#[test]
fn scope_matches_unnormalized_absolute_rule_path() {
    assert!(project_scope_matches(
        "/Users/vimes/projects/temet/temet-proxy/CLAUDE.md",
        "/Users/vimes/projects/temet/temet-proxy"
    ));
}

#[test]
fn scope_rejects_different_project() {
    assert!(!project_scope_matches(
        "/Users/vimes/projects/temet/other-proxy",
        "/Users/vimes/projects/temet/temet-proxy"
    ));
    assert!(!project_scope_matches(
        "projects/temet/other-proxy",
        "/Users/vimes/projects/temet/temet-proxy"
    ));
}

#[test]
fn scope_rejects_same_trimmed_tail_under_different_root() {
    assert!(!project_scope_matches(
        "/Users/someone-else/projects/temet/temet-proxy",
        "/Users/vimes/projects/temet/temet-proxy"
    ));
}

#[test]
fn scope_never_matches_empty_request_project() {
    assert!(!project_scope_matches(
        "/Users/vimes/projects/temet/temet-proxy",
        ""
    ));
    assert!(!project_scope_matches("projects/temet/temet-proxy", ""));
}

#[test]
fn scoped_rule_with_legacy_trimmed_path_fires_for_canonical_project() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    rule.project_path = Some("projects/temet/temet-proxy".to_string());
    let cfg = FilterConfig { rules: vec![rule] };
    let body = br#"{"system":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}"#;
    match apply_request_filter(body, &cfg, "/Users/vimes/projects/temet/temet-proxy") {
        FilterOutcome::Rewrote { .. } => Ok(()),
        FilterOutcome::Unchanged => Err(anyhow::anyhow!(
            "legacy trimmed scope must fire for the canonical project"
        )),
    }
}

#[test]
fn scoped_rule_for_other_project_does_not_fire() {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    rule.project_path = Some("projects/temet/other-proxy".to_string());
    let cfg = FilterConfig { rules: vec![rule] };
    let body = br#"{"system":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}"#;
    assert!(matches!(
        apply_request_filter(body, &cfg, "projects/temet/temet-proxy"),
        FilterOutcome::Unchanged
    ));
}
