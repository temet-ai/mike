use super::rule_test_support::take_err;
use super::{FilterAction, FilterKind, FilterRule, RuleSource};

#[test]
fn deserialize_inject_reminder_from_toml() -> anyhow::Result<()> {
    let toml_src = r##"
        kind = "inject_reminder"
        value = "chrome reminder"
        action = "replace"
        text = "Use tabs_context_mcp before guessing tab ids."
        trigger_token = "<chrome>"
    "##;
    let rule: FilterRule = toml::from_str(toml_src)?;
    assert_eq!(rule.kind, FilterKind::InjectReminder);
    assert_eq!(rule.trigger_token.as_deref(), Some("<chrome>"));
    Ok(())
}

#[test]
fn deserialize_lazy_section_from_toml() -> anyhow::Result<()> {
    let toml_src = r##"
        kind = "lazy_section"
        value = "# Claude in Chrome browser automation"
        action = "replace"
        text = "Stub: output !!CHROME!! to load the guidance."
        trigger_token = "!!CHROME!!"
    "##;
    let rule: FilterRule = toml::from_str(toml_src)?;
    assert_eq!(rule.kind, FilterKind::LazySection);
    assert_eq!(rule.trigger_token.as_deref(), Some("!!CHROME!!"));
    Ok(())
}

#[test]
fn deserialize_lazy_section_without_trigger_token_is_rejected() -> anyhow::Result<()> {
    let toml_src = r##"
        kind = "lazy_section"
        value = "# Heading"
        action = "replace"
        text = "stub"
    "##;
    let err = take_err(toml::from_str::<FilterRule>(toml_src))?;
    assert!(err.to_string().contains("`trigger_token`"), "got: {err}");
    Ok(())
}

#[test]
fn deserialize_drop_rule_from_toml_minimal() -> anyhow::Result<()> {
    let toml_src = r#"
        kind = "xml_tag"
        value = "system-reminder"
        action = "drop"
    "#;
    let rule: FilterRule = toml::from_str(toml_src)?;
    assert_eq!(rule.kind, FilterKind::XmlTag);
    assert_eq!(rule.value, "system-reminder");
    assert_eq!(rule.action, FilterAction::Drop);
    assert!(rule.enabled, "enabled defaults to true");
    assert_eq!(rule.source, RuleSource::Toml);
    assert!(rule.text.is_none());
    assert_eq!(
        rule.id, 0,
        "TOML-authored rules start with id=0 (unpersisted sentinel)"
    );
    assert_ne!(
        rule.uuid,
        uuid::Uuid::nil(),
        "uuid is auto-minted on missing field — should not be the nil UUID"
    );
    Ok(())
}

#[test]
fn deserialize_replace_rule_with_text() -> anyhow::Result<()> {
    let canonical_uuid = "550e8400-e29b-41d4-a716-446655440000";
    let toml_src = format!(
        r#"
        uuid = "{canonical_uuid}"
        kind = "text_substring"
        value = "skills are available"
        action = "replace"
        text = "Skills listed; invoke by name."
    "#
    );
    let rule: FilterRule = toml::from_str(&toml_src)?;
    assert_eq!(
        rule.id, 0,
        "TOML-authored rules start with id=0 (unpersisted sentinel)"
    );
    assert_eq!(rule.uuid.to_string(), canonical_uuid);
    assert_eq!(rule.action, FilterAction::Replace);
    assert_eq!(rule.text.as_deref(), Some("Skills listed; invoke by name."));
    Ok(())
}

#[test]
#[allow(
    clippy::expect_used,
    reason = "test fixture; non-UUID uuid is constant"
)]
fn deserialize_rejects_non_uuid_id() {
    let toml_src = r#"
        uuid = "my-custom-rule"
        kind = "xml_tag"
        value = "system-reminder"
        action = "drop"
    "#;
    let err =
        toml::from_str::<FilterRule>(toml_src).expect_err("non-UUID uuid must fail to deserialize");
    let msg = err.to_string();
    assert!(
        msg.contains("not a valid UUID"),
        "expected UUID error, got: {msg}"
    );
    assert!(
        msg.contains("my-custom-rule"),
        "error message must name the bad uuid: {msg}"
    );
    assert!(
        msg.contains("system-reminder"),
        "error message must include the matching `value` for locator clarity: {msg}"
    );
}

#[test]
#[allow(
    clippy::expect_used,
    reason = "test fixture; hard-coded UUID literals are constant"
)]
fn uuid_to_string_emits_canonical_lowercase_hyphenated() {
    let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
        .expect("hard-coded UUID literal must parse");
    assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    let id_upper = uuid::Uuid::parse_str("550E8400-E29B-41D4-A716-446655440000")
        .expect("hard-coded UUID literal must parse");
    assert_eq!(id_upper.to_string(), "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn deserialize_drop_with_text_is_rejected() -> anyhow::Result<()> {
    let toml_src = r#"
        kind = "xml_tag"
        value = "x"
        action = "drop"
        text = "nope"
    "#;
    let err = take_err(toml::from_str::<FilterRule>(toml_src))?;
    let msg = err.to_string();
    assert!(msg.contains("must not have a `text`"), "got: {msg}");
    Ok(())
}

#[test]
fn deserialize_replace_with_empty_text_is_rejected() -> anyhow::Result<()> {
    let toml_src = r#"
        kind = "text_substring"
        value = "x"
        action = "replace"
        text = ""
    "#;
    let err = take_err(toml::from_str::<FilterRule>(toml_src))?;
    let msg = err.to_string();
    assert!(msg.contains("non-empty"), "got: {msg}");
    Ok(())
}

#[test]
fn deserialize_append_without_text_is_rejected() -> anyhow::Result<()> {
    let toml_src = r#"
        kind = "text_substring"
        value = "x"
        action = "append"
    "#;
    let err = take_err(toml::from_str::<FilterRule>(toml_src))?;
    let msg = err.to_string();
    assert!(msg.contains("non-empty"), "got: {msg}");
    Ok(())
}
