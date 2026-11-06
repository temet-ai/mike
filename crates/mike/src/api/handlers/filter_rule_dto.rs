use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct FilterRuleDto {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) value: String,
    pub(crate) action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trigger_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) inject_text: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) source: crate::filter::RuleSource,
    pub(crate) updated_at: String,
    pub(crate) sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_path: Option<String>,
    pub(crate) last_fired_at: Option<String>,
    pub(crate) fires_24h: i64,
    pub(crate) fires_7d: i64,
    pub(crate) style_override: bool,
    pub(crate) title: Option<String>,
}

impl From<&crate::filter::FilterRule> for FilterRuleDto {
    fn from(r: &crate::filter::FilterRule) -> Self {
        use crate::filter::{FilterAction, FilterKind};
        let kind = match r.kind {
            FilterKind::TextSubstring => "text_substring",
            FilterKind::XmlTag => "xml_tag",
            FilterKind::LazySection => "lazy_section",
            FilterKind::InjectReminder => "inject_reminder",
        };
        let action = match r.action {
            FilterAction::Drop => "drop",
            FilterAction::Replace => "replace",
            FilterAction::Append => "append",
        };
        Self {
            id: r.id.to_string(),
            kind: kind.to_string(),
            value: r.value.clone(),
            action: action.to_string(),
            text: r.text.clone(),
            trigger_token: r.trigger_token.clone(),
            inject_text: r.inject_text.clone(),
            enabled: r.enabled,
            source: r.source,
            updated_at: r.updated_at.to_rfc3339(),
            sort_order: r.sort_order,
            project_path: r.project_path.clone(),
            last_fired_at: None,
            fires_24h: 0,
            fires_7d: 0,
            style_override: r.style_override,
            title: r.title.clone(),
        }
    }
}
