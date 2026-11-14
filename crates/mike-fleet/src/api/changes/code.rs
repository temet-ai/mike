use axum::Json;
use axum::extract::{Query, State};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ChangesError;
use crate::base_state::BaseState;
use crate::store::persistence::fleet_repo::FleetPersist as _;

const DEFAULT_HOURS: i64 = 24;
const MAX_HOURS: i64 = 24 * 30;
const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;

#[derive(Debug, Deserialize)]
pub struct CodeFindingsQuery {
    pub session: Option<String>,
    pub hours: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_code_findings(
    State(state): State<BaseState>,
    Query(query): Query<CodeFindingsQuery>,
) -> Result<Json<Value>, ChangesError> {
    let session = match query.session.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(
            raw.parse::<Uuid>()
                .map_err(|_| ChangesError::Invalid(format!("session {raw} is not a UUID")))?,
        ),
    };
    let hours = query.hours.unwrap_or(DEFAULT_HOURS).clamp(1, MAX_HOURS);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let since = Utc::now() - Duration::hours(hours);
    let persist = state.persist().ok_or(ChangesError::Unavailable)?;
    let (summary, rules) = persist.fleet().code_scan_summary(since).await?;
    let findings = persist
        .fleet()
        .list_code_findings(since, session, limit + 1)
        .await?;
    let truncated = findings.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    let findings: Vec<_> = findings
        .into_iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .collect();
    let session_counts = match session {
        Some(id) => Some(persist.fleet().session_code_counts(id).await?),
        None => None,
    };
    let toggles = state.feature_toggle_overlay.load();
    let rule_set = mike_code::RuleSet::builtin().ok();
    let scanning = toggles.change_record && toggles.code_scan;
    Ok(Json(json!({
        "scanning": scanning,
        "delivering": scanning && toggles.code_findings,
        "pushing": scanning && toggles.code_findings && toggles.code_findings_push,
        "grammars": mike_code::grammars(),
        "rules_version": rule_set.map(mike_code::RuleSet::version),
        "rule_ids": rule_set.map(mike_code::RuleSet::ids),
        "catalogue": rule_set.map(mike_code::RuleSet::catalogue),
        "project_rules": crate::change_record::code_scan::project_rules_status(),
        "watched_checkouts": crate::change_record::watch::watched_roots(),
        "session_counts": session_counts,
        "hours": hours,
        "summary": summary,
        "rules": rules,
        "findings": findings,
        "findings_truncated": truncated,
    })))
}
