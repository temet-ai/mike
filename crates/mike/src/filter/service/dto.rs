use super::super::FilterRule;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum OptionalField<T> {
    #[default]
    Absent,
    Null,
    Some(T),
}

#[derive(Debug)]
pub struct FilterRuleCreateInput {
    pub kind: String,
    pub value: String,
    pub action: String,
    pub text: Option<String>,
    pub trigger_token: Option<String>,
    pub inject_text: Option<String>,
    pub enabled: bool,
    pub project_path: Option<String>,
    pub style_override: bool,
    pub title: Option<String>,
}

#[derive(Debug, Default)]
pub struct FilterRulePatchInput {
    pub kind: Option<String>,
    pub value: Option<String>,
    pub action: Option<String>,
    pub text: OptionalField<String>,
    pub trigger_token: OptionalField<String>,
    pub inject_text: OptionalField<String>,
    pub enabled: Option<bool>,
    pub project_path: OptionalField<String>,
    pub sort_order: Option<i32>,
    pub style_override: Option<bool>,
    pub title: OptionalField<String>,
}

#[derive(Debug)]
pub struct FilterRuleEnvelopeOutput {
    pub rule: FilterRule,
    pub warnings: Vec<String>,
}
