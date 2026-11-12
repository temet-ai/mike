use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::bounded;
use crate::wright::Finding;
use crate::wright::definition::int_param;

const SQL: &str = include_str!("cold_forks.sql");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForkParams {
    pub(crate) window_hours: i64,
    pub(crate) cache_ttl_minutes: i64,
}

impl ForkParams {
    pub(crate) fn from_value(params: &Value) -> Result<Self, String> {
        Ok(Self {
            window_hours: int_param(params, "window_hours", 1, 168)?,
            cache_ttl_minutes: int_param(params, "cache_ttl_minutes", 1, 1440)?,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ColdFork {
    pub(crate) template: String,
    pub(crate) fork_session: Uuid,
    pub(crate) forked_at: DateTime<Utc>,
    pub(crate) fork_model: Option<String>,
    pub(crate) template_model: Option<String>,
    pub(crate) fork_args: Value,
    pub(crate) template_args: Value,
    pub(crate) first_request_id: i64,
    pub(crate) first_request_at: DateTime<Utc>,
    pub(crate) prompt_tokens: i64,
    pub(crate) template_last_request_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ColdCause {
    ModelChanged,
    PrefixChanged,
    TemplateNeverCached,
    CacheExpired,
    Unexplained,
}

impl ColdCause {
    pub(crate) fn code(self) -> &'static str {
        match self {
            ColdCause::ModelChanged => "model_changed",
            ColdCause::PrefixChanged => "prefix_changed",
            ColdCause::TemplateNeverCached => "template_never_cached",
            ColdCause::CacheExpired => "cache_expired",
            ColdCause::Unexplained => "unexplained",
        }
    }
}

fn ends_with_template(fork: &Value, template: &Value) -> bool {
    match (fork.as_array(), template.as_array()) {
        (Some(fork), Some(template)) => fork.ends_with(template),
        _ => false,
    }
}

fn models_differ(fork: &ColdFork) -> bool {
    matches!((&fork.fork_model, &fork.template_model), (Some(a), Some(b)) if a != b)
}

pub(crate) fn idle_gap(fork: &ColdFork) -> Option<Duration> {
    fork.template_last_request_at
        .map(|last| fork.first_request_at - last)
}

pub(crate) fn classify(fork: &ColdFork, cache_ttl_minutes: i64) -> ColdCause {
    if models_differ(fork) {
        return ColdCause::ModelChanged;
    }
    if !ends_with_template(&fork.fork_args, &fork.template_args) {
        return ColdCause::PrefixChanged;
    }
    match idle_gap(fork) {
        None => ColdCause::TemplateNeverCached,
        Some(gap) if gap > Duration::minutes(cache_ttl_minutes) => ColdCause::CacheExpired,
        Some(_) => ColdCause::Unexplained,
    }
}

fn fork_evidence(fork: &ColdFork) -> Value {
    json!({
        "fork_session": fork.fork_session,
        "forked_at": fork.forked_at,
        "first_request_id": fork.first_request_id,
        "prompt_tokens_written": fork.prompt_tokens,
        "template_idle_seconds": idle_gap(fork).map(|gap| gap.num_seconds()),
        "fork_model": fork.fork_model,
        "template_model": fork.template_model,
    })
}

pub(crate) fn findings_from(forks: &[ColdFork], params: ForkParams) -> Vec<Finding> {
    let mut grouped: BTreeMap<(String, ColdCause), Vec<Value>> = BTreeMap::new();
    for fork in forks {
        grouped
            .entry((
                fork.template.clone(),
                classify(fork, params.cache_ttl_minutes),
            ))
            .or_default()
            .push(fork_evidence(fork));
    }
    grouped
        .into_iter()
        .map(|((template, cause), forks)| Finding {
            scope: "fork".to_owned(),
            resource: format!("{template}: {}", cause.code()),
            evidence: json!({
                "window": format!("{} hours", params.window_hours),
                "template": template,
                "cause": cause.code(),
                "cache_ttl_minutes": params.cache_ttl_minutes,
                "cold_forks": forks.len(),
                "forks": forks,
            }),
        })
        .collect()
}

pub(crate) async fn check(
    pool: &PgPool,
    name: &str,
    params: &Value,
) -> Result<Vec<Finding>, String> {
    let parsed = ForkParams::from_value(params)?;
    let forks: Vec<ColdFork> = bounded::rows(pool, name, SQL, params).await?;
    Ok(findings_from(&forks, parsed))
}
