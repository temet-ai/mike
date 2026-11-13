use super::*;

#[derive(Debug, Serialize)]
pub struct FilterRuleDto {
    pub id: String,
    pub kind: String,
    pub value: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inject_text: Option<String>,
    pub enabled: bool,
    pub source: crate::filter::RuleSource,
    pub updated_at: String,
    pub sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub last_fired_at: Option<String>,
    pub fires_24h: i64,
    pub fires_7d: i64,
    pub style_override: bool,
    pub title: Option<String>,
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
