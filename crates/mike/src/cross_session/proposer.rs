use serde::Serialize;

use crate::settings::cross_session::CrossSessionSettings;
use crate::store::persistence::cross_session::ProposerParams;
use crate::store::persistence::{PersistError, PersistHandle};

pub const PROPOSER_VERSION: &str = "proposer-v1";
pub const REJECT_COOLDOWN_DAYS: i32 = 30;
pub const MAX_NEW_PER_RUN: i64 = 3;
pub const MAX_OPEN: i64 = 10;
pub const OPERATOR_MIN_SCOPES: i64 = 3;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProposerReport {
    pub repeated_directive: u64,
    pub confirmed_handback: u64,
    pub operator_rollups: u64,
    pub sources_unavailable: Vec<String>,
}

pub fn params<'a>(settings: &CrossSessionSettings, rollup_id: i64) -> ProposerParams<'a> {
    ProposerParams {
        window_days: i32::try_from(settings.proposer_window_days).unwrap_or(30),
        min_sessions: i32::try_from(settings.min_sessions).unwrap_or(2),
        min_confirmed: i32::try_from(settings.min_confirmed).unwrap_or(3),
        rollup_id,
        version: PROPOSER_VERSION,
        reject_cooldown_days: REJECT_COOLDOWN_DAYS,
        max_new_per_run: MAX_NEW_PER_RUN,
        max_open: MAX_OPEN,
        operator_min_scopes: OPERATOR_MIN_SCOPES,
    }
}

pub async fn run_proposer_pass(
    persist: &PersistHandle,
    settings: &CrossSessionSettings,
    rollup_id: i64,
) -> Result<ProposerReport, PersistError> {
    let p = params(settings, rollup_id);
    let mut report = ProposerReport::default();
    if settings.source_enabled("repeated_directive") {
        report.repeated_directive = persist.propose_repeated_directives(&p).await?;
    }
    if settings.source_enabled("confirmed_handback") {
        report.confirmed_handback = persist.propose_confirmed_handbacks(&p).await?;
    }
    for unavailable in ["recurring_failure_tag", "recurring_pattern"] {
        if settings.source_enabled(unavailable) {
            report.sources_unavailable.push(unavailable.to_owned());
        }
    }
    report.operator_rollups = persist.roll_up_operator_scope(&p).await?;
    Ok(report)
}
