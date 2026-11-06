use chrono::{DateTime, Utc};

use crate::dossier::render::render;
use crate::dossier::view::types::{AsOf, Audience, Budget, SessionSlices};
use crate::llm::{CallUsage, LlmRuntimeSettings, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_end::SessionEndFacts;

use super::config::SessionJudgeV3Config;
use super::defaults::{EndVerdict, apply_rules};
use super::rubric::system_prompt;
use super::schema::{SessionVerdictV3, session_verdict_v3_schema};
use super::{EndRule, SessionEndLabel, normalise_tags};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSubject {
    pub session_db_id: i64,
    pub as_of: AsOf,
}

#[derive(Debug, Clone)]
pub struct SessionJudgementV3 {
    pub verdict: SessionVerdictV3,
    pub facts: Option<SessionEndFacts>,
    pub rule_fired: Option<EndRule>,
    pub end: Option<EndVerdict>,
    pub input_text: String,
    pub digest_version: String,
    pub as_of: DateTime<Utc>,
    pub usage: CallUsage,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionJudgeFailure {
    #[error("the budget refused the call")]
    Refused,
    #[error("the model call failed: {0}")]
    Transport(String),
    #[error("the dossier could not be rendered: {0}")]
    Render(String),
    #[error("the session facts could not be read: {0}")]
    Facts(String),
    #[error("the bundle names an unknown rubric")]
    Rubric,
}

fn llm_end(v: &SessionVerdictV3) -> Option<EndVerdict> {
    let label = SessionEndLabel::parse(&v.end_label)?;
    let confidence = match v.end_confidence.as_str() {
        "high" => "high",
        "medium" => "medium",
        _ => "low",
    };
    let severity = match v.severity.as_str() {
        "minor" => "minor",
        "major" => "major",
        "severe" => "severe",
        _ => "none",
    };
    Some(EndVerdict {
        label,
        deterministic: None,
        confidence,
        evidence: v.end_evidence.clone(),
        failure: v.failure.clone(),
        failure_tags: normalise_tags(&v.failure_tags),
        severity,
    })
}

pub fn effective_end(
    facts: Option<&SessionEndFacts>,
    verdict: &SessionVerdictV3,
    rules: &[EndRule],
) -> (Option<EndRule>, Option<EndVerdict>) {
    let Some(f) = facts.filter(|f| f.settled()) else {
        return (None, None);
    };
    let (fired, applied) = apply_rules(f, rules);
    (fired, applied.or_else(|| llm_end(verdict)))
}

async fn digest(
    persist: &PersistHandle,
    cfg: &SessionJudgeV3Config,
    subject: &SessionSubject,
) -> Result<(String, String), SessionJudgeFailure> {
    let audience = Audience::SessionJudge {
        slices: SessionSlices::default(),
    };
    let rendered = render(
        persist,
        subject.session_db_id,
        subject.as_of,
        &audience,
        Budget::Chars(cfg.digest_budget_chars),
    )
    .await
    .map_err(|e| SessionJudgeFailure::Render(e.to_string()))?;
    Ok((rendered.text.clone(), rendered.digest_version()))
}

fn configured(
    llm: &OpenRouterClient,
    cfg: &SessionJudgeV3Config,
) -> Result<OpenRouterClient, SessionJudgeFailure> {
    let defaults = LlmRuntimeSettings::default();
    let settings = LlmRuntimeSettings {
        reasoning_effort: cfg
            .reasoning_effort
            .clone()
            .unwrap_or(defaults.reasoning_effort.clone()),
        max_output_tokens: cfg.max_output_tokens.unwrap_or(defaults.max_output_tokens),
        ..defaults
    };
    let llm = if llm.model() == cfg.model {
        llm.clone()
    } else {
        llm.with_model(cfg.model.clone())
            .map_err(|e| SessionJudgeFailure::Transport(e.to_string()))?
    };
    Ok(llm.with_runtime_settings(settings))
}

fn log_provider_unavailable(e: &crate::llm::LlmError) {
    if matches!(e, crate::llm::LlmError::ProviderUnavailable { .. }) {
        tracing::warn!(error = %e, "session judge: refused before any call was made");
    }
}

pub async fn judge_session_v3(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    cfg: &SessionJudgeV3Config,
    subject: &SessionSubject,
) -> Result<SessionJudgementV3, SessionJudgeFailure> {
    let facts = persist
        .fetch_session_end_facts(&[subject.session_db_id], subject.as_of.at)
        .await
        .map_err(|e| SessionJudgeFailure::Facts(e.to_string()))?
        .into_iter()
        .next();
    let (input_text, digest_version) = digest(persist, cfg, subject).await?;
    let system = system_prompt(cfg.rubric_id).ok_or(SessionJudgeFailure::Rubric)?;
    let llm = configured(llm, cfg)?;
    let (verdict, usage) = llm
        .verdict_with_usage::<SessionVerdictV3>(system, &input_text, session_verdict_v3_schema())
        .await
        .map_err(|e| {
            if e.is_refusal() {
                log_provider_unavailable(&e);
                SessionJudgeFailure::Refused
            } else {
                SessionJudgeFailure::Transport(e.to_string())
            }
        })?;
    let (rule_fired, end) = effective_end(facts.as_ref(), &verdict, &cfg.default_rules);
    Ok(SessionJudgementV3 {
        verdict,
        facts,
        rule_fired,
        end,
        input_text,
        digest_version,
        as_of: subject.as_of.at,
        usage,
    })
}
