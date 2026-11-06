use chrono::Utc;

use super::{FilterAction, FilterKind, FilterRule, RuleSource};

pub(super) fn new_drop_rule(kind: FilterKind, value: &str) -> FilterRule {
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

pub(super) fn take_err<T, E>(result: Result<T, E>) -> anyhow::Result<E> {
    match result {
        Ok(_) => Err(anyhow::anyhow!("expected Err, got Ok")),
        Err(e) => Ok(e),
    }
}

pub(super) fn new_lazy_rule() -> FilterRule {
    FilterRule {
        id: 0,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::LazySection,
        value: "# Claude in Chrome browser automation".to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: Utc::now(),
        sort_order: 0,
        action: FilterAction::Replace,
        text: Some("Stub: output !!CHROME!! to load the guidance.".to_string()),
        trigger_token: Some("!!CHROME!!".to_string()),
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

pub(super) fn new_inject_reminder_rule() -> FilterRule {
    FilterRule {
        id: 0,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::InjectReminder,
        value: "chrome reminder".to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: Utc::now(),
        sort_order: 0,
        action: FilterAction::Replace,
        text: Some("Use tabs_context_mcp before guessing tab ids.".to_string()),
        trigger_token: Some("<chrome>".to_string()),
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}
