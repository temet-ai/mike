use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::model_ref::{ModelRef, Provider};
use crate::store::persistence::lessons::maintenance_budget::MaintenanceRefusal;
use crate::store::persistence::llm_role_health::LlmRoleOutcome;
use chrono::{DateTime, Duration, Utc};

mod maintenance;

const FAILING_STREAK: i64 = 3;
const WINDOW_DAYS: i64 = 7;
const TOP_UP_URL: &str = "https://openrouter.ai/settings/credits";
const MODEL_SETTINGS: &str = "/settings/steering";

struct MonitoredRole {
    db_role: &'static str,
    label: &'static str,
    silence_hours: i64,
    toggle: Option<fn(&FeatureToggles) -> bool>,
}

const ROLES: [MonitoredRole; 5] = [
    MonitoredRole {
        db_role: "steerer",
        label: "Steerers",
        silence_hours: 48,
        toggle: Some(|t| t.steerer),
    },
    MonitoredRole {
        db_role: "judge_worker",
        label: "Turn judge",
        silence_hours: 48,
        toggle: Some(|t| t.judge_worker),
    },
    MonitoredRole {
        db_role: "session_judge",
        label: "Session judge",
        silence_hours: 48,
        toggle: Some(|t| t.session_judge),
    },
    MonitoredRole {
        db_role: "lesson_proposer",
        label: "Lesson proposer",
        silence_hours: 72,
        toggle: Some(|t| t.judge_lessons),
    },
    MonitoredRole {
        db_role: "lesson_relations",
        label: "Lesson relations",
        silence_hours: 0,
        toggle: None,
    },
];

struct Finding {
    state: CapabilityState,
    detail: String,
    fix: String,
}

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: "llm_role_health",
        label: "LLM worker roles",
        state,
        detail,
        fix,
    }
}

fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn span(duration: Duration) -> String {
    let hours = duration.num_hours();
    if hours >= 48 {
        format!("{} days", hours / 24)
    } else {
        format!("{hours} hours")
    }
}

fn failure_reason(outcome: &LlmRoleOutcome) -> String {
    match outcome.last_http_status {
        Some(402) => "HTTP 402 Payment Required".to_owned(),
        Some(code) => format!("HTTP {code}"),
        None => outcome.last_outcome.clone(),
    }
}

fn provider_of(model: &str) -> Provider {
    ModelRef::parse(model).provider
}

fn failure_fix(outcome: &LlmRoleOutcome) -> String {
    match outcome.last_http_status {
        Some(402) => format!(
            "Add credit at {TOP_UP_URL}, or set the role's model to a codex/ model at {MODEL_SETTINGS}."
        ),
        Some(401 | 403) => format!(
            "The provider rejected the credentials. Check the OpenRouter API key, or run `codex login` for a codex/ model or `claude auth login` for a claude/ model, then check the role's model at {MODEL_SETTINGS}."
        ),
        None if provider_of(&outcome.model) == Provider::Codex => format!(
            "Run `codex login status` on the machine that runs the proxy and log in again if it is signed out, or set the role's model to an OpenRouter model at {MODEL_SETTINGS}."
        ),
        None if provider_of(&outcome.model) == Provider::Claude => format!(
            "Run `claude auth status` on the machine that runs the proxy and run `claude auth login` if it reports loggedIn false, or set the role's model to an OpenRouter model at {MODEL_SETTINGS}."
        ),
        _ => format!(
            "Open the failing calls on the Judges page (or read llm.calls) and check the role's model at {MODEL_SETTINGS}."
        ),
    }
}

fn failing_finding(role: &MonitoredRole, outcome: &LlmRoleOutcome) -> Finding {
    let last_success = outcome.last_ok_at.map_or_else(
        || format!("no call succeeded in the last {WINDOW_DAYS} days"),
        |at| format!("last success {}", stamp(at)),
    );
    let state = if outcome.failed_streak >= FAILING_STREAK {
        CapabilityState::Inert
    } else {
        CapabilityState::Degraded
    };
    Finding {
        state,
        detail: format!(
            "{}: the last {} call(s) failed, newest {} on {} ({}); {last_success}.",
            role.label,
            outcome.failed_streak,
            stamp(outcome.last_call_at),
            outcome.model,
            failure_reason(outcome),
        ),
        fix: failure_fix(outcome),
    }
}

fn silent_finding(
    role: &MonitoredRole,
    outcome: Option<&LlmRoleOutcome>,
    now: DateTime<Utc>,
) -> Option<Finding> {
    let silence = match outcome {
        None => format!("in the last {WINDOW_DAYS} days"),
        Some(o) => {
            let quiet = now - o.last_call_at;
            if quiet <= Duration::hours(role.silence_hours) {
                return None;
            }
            format!("for {} (last call {})", span(quiet), stamp(o.last_call_at))
        }
    };
    Some(Finding {
        state: CapabilityState::Degraded,
        detail: format!(
            "{} is switched on but has made no LLM call {silence}.",
            role.label
        ),
        fix: format!(
            "Check the proxy log for the worker's last error, the LLM worker ownership row for a lease held by another machine, and the role's model at {MODEL_SETTINGS}."
        ),
    })
}

fn assess(
    role: &MonitoredRole,
    outcome: Option<&LlmRoleOutcome>,
    toggles: &FeatureToggles,
    now: DateTime<Utc>,
) -> Option<Finding> {
    if let Some(o) = outcome.filter(|o| o.last_outcome != "ok") {
        return Some(failing_finding(role, o));
    }
    if role.toggle.is_some_and(|enabled| enabled(toggles)) {
        return silent_finding(role, outcome, now);
    }
    None
}

fn healthy_detail(outcomes: &[LlmRoleOutcome]) -> String {
    let answered: Vec<String> = ROLES
        .iter()
        .filter_map(|role| {
            outcomes
                .iter()
                .find(|o| o.role == role.db_role)
                .map(|o| format!("{} ({})", role.label, stamp(o.last_call_at)))
        })
        .collect();
    if answered.is_empty() {
        return format!(
            "No LLM worker call in the last {WINDOW_DAYS} days, and no switched-on worker is overdue for one"
        );
    }
    format!("The most recent call succeeded for {}", answered.join(", "))
}

fn merged_fix(findings: &[Finding]) -> String {
    let mut fixes: Vec<&str> = Vec::new();
    for finding in findings {
        if !fixes.contains(&finding.fix.as_str()) {
            fixes.push(&finding.fix);
        }
    }
    fixes.join(" ")
}

fn role_findings(
    now: DateTime<Utc>,
    outcomes: &[LlmRoleOutcome],
    toggles: &FeatureToggles,
) -> Vec<Finding> {
    ROLES
        .iter()
        .filter_map(|role| {
            let outcome = outcomes.iter().find(|o| o.role == role.db_role);
            assess(role, outcome, toggles, now)
        })
        .collect()
}

fn provider_findings(provider: &ProviderStatus) -> Vec<Finding> {
    provider
        .problems
        .iter()
        .map(|problem| Finding {
            state: CapabilityState::Inert,
            detail: problem.detail.clone(),
            fix: problem.fix.clone(),
        })
        .collect()
}

pub(crate) fn llm_role_health_row(
    now: DateTime<Utc>,
    outcomes: &[LlmRoleOutcome],
    refusals: &[MaintenanceRefusal],
    toggles: &FeatureToggles,
    provider: &ProviderStatus,
) -> CapabilityRow {
    let mut findings = provider_findings(provider);
    findings.extend(role_findings(now, outcomes, toggles));
    findings.extend(maintenance::findings(refusals));
    let heading = provider.heading();
    if findings.is_empty() {
        return row(
            CapabilityState::Active,
            format!("{heading} {}", healthy_detail(outcomes)),
            String::new(),
        );
    }
    let state = if findings.iter().any(|f| f.state == CapabilityState::Inert) {
        CapabilityState::Inert
    } else {
        CapabilityState::Degraded
    };
    let detail = findings
        .iter()
        .map(|f| f.detail.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    row(state, format!("{heading} {detail}"), merged_fix(&findings))
}

pub(crate) async fn llm_role_health_capability(
    state: &AppState,
    toggles: &FeatureToggles,
) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is disabled, so LLM call outcomes are not recorded".to_owned(),
            String::new(),
        );
    };
    let provider = provider_status(state, toggles).await;
    let refusals = persist
        .recent_maintenance_refusals()
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "capabilities: lesson maintenance refusals could not be read");
            Vec::new()
        });
    match persist.llm_role_outcomes().await {
        Ok(outcomes) => llm_role_health_row(Utc::now(), &outcomes, &refusals, toggles, &provider),
        Err(e) => row(
            CapabilityState::Degraded,
            format!("Could not read the LLM worker call outcomes: {e}"),
            "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests;
