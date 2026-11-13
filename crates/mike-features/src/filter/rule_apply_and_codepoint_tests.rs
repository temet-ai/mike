use super::rule_test_support::{new_drop_rule, take_err};
use super::{FilterAction, FilterConfig, FilterKind, FilterOutcome, apply_request_filter};

#[test]
fn empty_rule_list_is_unchanged() {
    let cfg = FilterConfig::default();
    let body = br#"{"system":[{"type":"text","text":"hi"}]}"#;
    assert!(matches!(
        apply_request_filter(body, &cfg, ""),
        FilterOutcome::Unchanged
    ));
}

#[test]
fn only_disabled_rules_is_unchanged() {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    rule.enabled = false;
    let cfg = FilterConfig { rules: vec![rule] };
    let body = br#"{"system":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}"#;
    assert!(matches!(
        apply_request_filter(body, &cfg, ""),
        FilterOutcome::Unchanged
    ));
}

#[test]
fn validate_rejects_nul_in_value() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::TextSubstring, "before\x00after");
    let err = take_err(rule.validate())?;
    assert!(err.contains("invalid character U+0000"), "got: {err}");
    assert!(err.contains("`value`"), "error names the field: {err}");
    Ok(())
}

#[test]
fn validate_rejects_c0_control_in_value() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::TextSubstring, "bell\x07here");
    let err = take_err(rule.validate())?;
    assert!(err.contains("U+0007"), "got: {err}");
    Ok(())
}

#[test]
fn validate_accepts_tab_cr_lf_in_value() {
    let rule = new_drop_rule(FilterKind::TextSubstring, "tab\there\nand\rline");
    assert!(
        rule.validate().is_ok(),
        "\\t, \\n, \\r are explicitly allowed"
    );
}

#[test]
fn validate_rejects_bidi_override_in_value() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::TextSubstring, "admin\u{202E}boss");
    let err = take_err(rule.validate())?;
    assert!(err.contains("U+202E"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_unsafe_codepoint_in_text() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::TextSubstring, "value-ok");
    rule.action = FilterAction::Replace;
    rule.text = Some("hello\u{202A}world".to_string());
    let err = take_err(rule.validate())?;
    assert!(err.contains("U+202A"), "got: {err}");
    assert!(err.contains("`text`"), "error names the field: {err}");
    Ok(())
}

#[test]
fn validate_accepts_ordinary_xml_tag_value() {
    let rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    assert!(rule.validate().is_ok());
}

#[test]
fn validate_accepts_xml_tag_with_underscore_colon_dot() {
    for value in ["_foo", ":ns", "foo.bar", "foo:bar", "a_b-c.d"] {
        let rule = new_drop_rule(FilterKind::XmlTag, value);
        assert!(
            rule.validate().is_ok(),
            "value {value:?} should validate as an element name"
        );
    }
}

#[test]
fn validate_rejects_xml_tag_containing_angle_bracket() -> anyhow::Result<()> {
    for bad in ["<foo", "foo>", "<foo>", "foo<bar"] {
        let rule = new_drop_rule(FilterKind::XmlTag, bad);
        let err = take_err(rule.validate())?;
        assert!(
            err.contains("xml_tag"),
            "rejection names the kind for {bad:?}: {err}"
        );
    }
    Ok(())
}

#[test]
fn validate_rejects_xml_tag_containing_slash() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::XmlTag, "foo/bar");
    let err = take_err(rule.validate())?;
    assert!(err.contains("invalid character"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_xml_tag_starting_with_digit() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::XmlTag, "1foo");
    let err = take_err(rule.validate())?;
    assert!(err.contains("must start with"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_xml_tag_with_space_attribute_style() -> anyhow::Result<()> {
    let rule = new_drop_rule(FilterKind::XmlTag, "foo bar");
    let err = take_err(rule.validate())?;
    assert!(err.contains("invalid character"), "got: {err}");
    Ok(())
}

#[test]
fn text_substring_value_is_not_subject_to_xml_name_check() {
    let rule = new_drop_rule(FilterKind::TextSubstring, "<foo bar>/weird");
    assert!(rule.validate().is_ok());
}
