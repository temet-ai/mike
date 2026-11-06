use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct FilterConfig {
    #[serde(default)]
    pub rules: Vec<FilterRule>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FilterRule {
    pub id: i64,

    pub uuid: uuid::Uuid,

    pub kind: FilterKind,

    pub value: String,

    pub enabled: bool,

    pub source: RuleSource,

    pub updated_at: DateTime<Utc>,

    pub sort_order: i32,

    pub action: FilterAction,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub inject_text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,

    pub style_override: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    TextSubstring,
    XmlTag,
    LazySection,
    InjectReminder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    Drop,
    Replace,
    Append,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    #[default]
    Toml,
    Ui,
}

#[derive(Debug)]
pub(crate) enum FilterOutcome {
    Unchanged,
    Rewrote {
        new_body: Bytes,
        original_len: usize,
        filtered_len: usize,
        firing_rule_ids: Vec<i64>,
    },
}
