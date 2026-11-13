use super::rule_test_support::{new_drop_rule, new_inject_reminder_rule, new_lazy_rule, take_err};
use super::{FilterAction, FilterKind};

#[test]
fn stored_rules_load_despite_banned_vocabulary_but_authoring_them_is_refused() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("response-style.md");
    std::fs::write(
        &path,
        "The following words and their relatives are banned in anything you \
         write for me: load-bearing, gate, clean (as a verdict), robust.",
    )?;
    unsafe {
        std::env::set_var("MIKE_STYLE_RULES_PATH", &path);
    }

    let mut rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    rule.action = FilterAction::Replace;
    rule.text = Some("Before claiming done, cite the observable. Keep it clean.".to_string());

    let read_back = rule.validate_structure();
    let authored = rule.validate();

    unsafe {
        std::env::remove_var("MIKE_STYLE_RULES_PATH");
    }

    assert!(
        read_back.is_ok(),
        "a stored rule must load regardless of its vocabulary; this is what \
         stopped the proxy booting: {read_back:?}"
    );
    let err = authored
        .err()
        .ok_or_else(|| anyhow::anyhow!("authoring banned vocabulary must still be refused"))?;
    assert!(
        err.contains("clean"),
        "the authoring error must name the term it objected to: {err}"
    );
    Ok(())
}

#[test]
fn validate_accepts_drop_with_no_text() {
    let rule = new_drop_rule(FilterKind::XmlTag, "system-reminder");
    assert!(rule.validate().is_ok());
}

#[test]
fn validate_rejects_drop_with_text() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "x");
    rule.text = Some("should not be here".to_string());
    let err = take_err(rule.validate())?;
    assert!(err.contains("must not have a `text`"));
    Ok(())
}

#[test]
fn validate_rejects_replace_with_empty_text() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "x");
    rule.action = FilterAction::Replace;
    rule.text = Some(String::new());
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty"));
    Ok(())
}

#[test]
fn validate_rejects_append_missing_text() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "x");
    rule.action = FilterAction::Append;
    rule.text = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty"));
    Ok(())
}

#[test]
fn validate_rejects_empty_value() -> anyhow::Result<()> {
    let mut rule = new_drop_rule(FilterKind::XmlTag, "");
    rule.text = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("`value`"));
    Ok(())
}

#[test]
fn validate_accepts_well_formed_lazy_section() {
    assert!(new_lazy_rule().validate().is_ok());
}

#[test]
fn validate_rejects_lazy_section_missing_text() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.text = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty `text`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_lazy_section_empty_text() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.text = Some(String::new());
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty `text`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_lazy_section_missing_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.trigger_token = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("`trigger_token`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_lazy_section_empty_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.trigger_token = Some(String::new());
    let err = take_err(rule.validate())?;
    assert!(err.contains("`trigger_token`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_lazy_section_non_replace_action() -> anyhow::Result<()> {
    for action in [FilterAction::Drop, FilterAction::Append] {
        let mut rule = new_lazy_rule();
        rule.action = action;
        if action == FilterAction::Drop {
            rule.text = Some("still set — the action gate fires first".to_string());
        }
        let err = take_err(rule.validate())?;
        assert!(err.contains("requires action=replace"), "got: {err}");
    }
    Ok(())
}

#[test]
fn validate_rejects_lazy_section_empty_value() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.value = String::new();
    let err = take_err(rule.validate())?;
    assert!(err.contains("`value`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_trigger_token_on_other_kinds() -> anyhow::Result<()> {
    for kind in [FilterKind::TextSubstring, FilterKind::XmlTag] {
        let mut rule = new_drop_rule(kind, "needle");
        rule.trigger_token = Some("!!CHROME!!".to_string());
        let err = take_err(rule.validate())?;
        assert!(
            err.contains("only valid on kind=lazy_section"),
            "got: {err}"
        );
    }
    Ok(())
}

#[test]
fn validate_accepts_well_formed_inject_reminder() {
    assert!(new_inject_reminder_rule().validate().is_ok());
}

#[test]
fn validate_rejects_inject_reminder_missing_text() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.text = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty `text`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_inject_reminder_empty_text() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.text = Some(String::new());
    let err = take_err(rule.validate())?;
    assert!(err.contains("non-empty `text`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_inject_reminder_missing_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.trigger_token = None;
    let err = take_err(rule.validate())?;
    assert!(err.contains("`trigger_token`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_inject_reminder_empty_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.trigger_token = Some(String::new());
    let err = take_err(rule.validate())?;
    assert!(err.contains("`trigger_token`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_inject_reminder_empty_value() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.value = String::new();
    let err = take_err(rule.validate())?;
    assert!(err.contains("`value`"), "got: {err}");
    Ok(())
}

#[test]
fn validate_rejects_inject_reminder_non_replace_action() -> anyhow::Result<()> {
    for action in [FilterAction::Drop, FilterAction::Append] {
        let mut rule = new_inject_reminder_rule();
        rule.action = action;
        let err = take_err(rule.validate())?;
        assert!(err.contains("requires action=replace"), "got: {err}");
    }
    Ok(())
}

#[test]
fn validate_rejects_unsafe_codepoint_in_inject_reminder_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_inject_reminder_rule();
    rule.trigger_token = Some("tok\u{202E}en".to_string());
    let err = take_err(rule.validate())?;
    assert!(err.contains("U+202E"), "got: {err}");
    assert!(
        err.contains("`trigger_token`"),
        "error names the field: {err}"
    );
    Ok(())
}

#[test]
fn validate_rejects_unsafe_codepoint_in_trigger_token() -> anyhow::Result<()> {
    let mut rule = new_lazy_rule();
    rule.trigger_token = Some("tok\u{202E}en".to_string());
    let err = take_err(rule.validate())?;
    assert!(err.contains("U+202E"), "got: {err}");
    assert!(
        err.contains("`trigger_token`"),
        "error names the field: {err}"
    );
    Ok(())
}
