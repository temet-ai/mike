use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::compose_prompt::PromptClassKey;
use crate::filter::{FilterConfig, FilterKind, FilterRule, project_scope_matches};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ProjectQuery {
    #[serde(default)]
    project: String,
}

#[derive(Debug, Serialize)]
pub struct ShapingRule {
    id: i64,
    kind: FilterKind,
    value: String,
    action: crate::filter::FilterAction,
    text: Option<String>,
    #[serde(rename = "match")]
    match_: &'static str,
    scope: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ShapingLazySection {
    id: i64,
    kind: FilterKind,
    heading: Option<String>,
    stub: Option<String>,
    trigger_token: Option<String>,
    inject_text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ShapingComposition {
    id: i64,
    prompt_sha256: String,
    mode: &'static str,
    lane: String,
    class: String,
    blocks: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub struct PromptShaping {
    filter_rules: Vec<ShapingRule>,
    lazy_sections: Vec<ShapingLazySection>,
    compositions: Vec<ShapingComposition>,
}

fn applicable<'a>(cfg: &'a FilterConfig, project: &'a str) -> impl Iterator<Item = &'a FilterRule> {
    cfg.rules.iter().filter(move |r| {
        r.enabled
            && r.project_path
                .as_deref()
                .is_none_or(|p| project_scope_matches(p, project))
    })
}

fn shaping_rule(rule: &FilterRule) -> Option<ShapingRule> {
    let match_ = match rule.kind {
        FilterKind::TextSubstring => "substring_ci",
        FilterKind::XmlTag => "xml_block",
        FilterKind::LazySection | FilterKind::InjectReminder => return None,
    };
    Some(ShapingRule {
        id: rule.id,
        kind: rule.kind,
        value: rule.value.clone(),
        action: rule.action,
        text: rule.text.clone(),
        match_,
        scope: "system_and_messages",
    })
}

fn shaping_lazy(rule: &FilterRule) -> Option<ShapingLazySection> {
    let heading = match rule.kind {
        FilterKind::LazySection => Some(rule.value.clone()),
        FilterKind::InjectReminder => None,
        FilterKind::TextSubstring | FilterKind::XmlTag => return None,
    };
    Some(ShapingLazySection {
        id: rule.id,
        kind: rule.kind,
        heading,
        stub: rule.text.clone(),
        trigger_token: rule.trigger_token.clone(),
        inject_text: rule.inject_text.clone(),
    })
}

fn class_name(key: &PromptClassKey) -> String {
    match key {
        PromptClassKey::Main => "main".to_owned(),
        PromptClassKey::Subagent(persona) => format!("subagent:{persona}"),
    }
}

pub fn build(state: &AppState, project: &str) -> PromptShaping {
    let cfg = state.filter_config.load();
    let mut compositions: Vec<ShapingComposition> = state
        .composer
        .cache()
        .load()
        .iter()
        .map(|(key, c)| ShapingComposition {
            id: c.id,
            prompt_sha256: c.prompt_sha256.clone(),
            mode: c.mode.as_str(),
            lane: c.lane.clone(),
            class: class_name(key),
            blocks: c.blocks.clone(),
        })
        .collect();
    compositions.sort_by_key(|c| c.id);
    PromptShaping {
        filter_rules: applicable(&cfg, project).filter_map(shaping_rule).collect(),
        lazy_sections: applicable(&cfg, project).filter_map(shaping_lazy).collect(),
        compositions,
    }
}

pub async fn get_prompt_shaping(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<PromptShaping>, StatusCode> {
    if Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    crate::api::client_serves::require_client_serves(&state, "client_serves_prompt_shaping")?;
    Ok(Json(build(&state, &query.project)))
}

#[cfg(test)]
mod tests;
