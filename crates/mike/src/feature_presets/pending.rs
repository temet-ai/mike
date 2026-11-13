use std::path::Path;

use super::apply::apply_via_services;
use super::plan::{Preset, plan};
use crate::finding::Finding;
use crate::state::AppState;
use crate::user_config::{ConfigFile, SettingValue, UserConfigError};

pub(crate) const PENDING_KEY: &str = "setup_features";
const PENDING_ENV: &str = "MIKE_SETUP_FEATURES";

pub(crate) fn save_pending(config_file: &Path, preset: Preset) -> Result<(), UserConfigError> {
    let mut file = ConfigFile::open(config_file)?;
    file.set(PENDING_KEY, SettingValue::Text(preset.as_str()))?;
    file.save()
}

fn clear_pending(config_file: &Path) -> Result<(), UserConfigError> {
    let mut file = ConfigFile::open(config_file)?;
    file.remove(PENDING_KEY);
    file.save()
}

pub(crate) fn pending_preset(value: Option<&str>) -> Option<Preset> {
    value.and_then(Preset::parse)
}

async fn apply_now(state: &AppState, preset: Preset) -> Option<Vec<Finding>> {
    let (Some(toggles), Some(settings)) =
        (state.feature_toggle_service(), state.settings_service())
    else {
        tracing::warn!(
            preset = preset.as_str(),
            "setup: the saved feature choice waits for a database, which this start does not have"
        );
        return None;
    };
    Some(apply_via_services(toggles, settings, &plan(preset)).await)
}

fn report_failures(preset: Preset, findings: &[Finding]) -> bool {
    let failures: Vec<&Finding> = findings.iter().filter(|f| !f.passed()).collect();
    for finding in &failures {
        tracing::warn!(
            preset = preset.as_str(),
            problem = %finding.text,
            "setup: could not apply the saved feature choice"
        );
    }
    !failures.is_empty()
}

fn forget_choice(config_file: &Path) {
    if let Err(e) = clear_pending(config_file) {
        tracing::warn!(error = %e, "setup: could not clear the saved feature choice");
    }
    unsafe {
        std::env::remove_var(PENDING_ENV);
    }
}

pub(crate) async fn apply_pending_at_start(state: &AppState) {
    let config = state.config();
    let selected = std::env::var(PENDING_ENV).ok().or_else(|| {
        ConfigFile::open(&config.config_path)
            .ok()
            .and_then(|file| file.literal(PENDING_ENV).ok().flatten())
    });
    let Some(preset) = pending_preset(selected.as_deref()) else {
        return;
    };
    let Some(findings) = apply_now(state, preset).await else {
        return;
    };
    if report_failures(preset, &findings) {
        return;
    }
    tracing::info!(
        preset = preset.as_str(),
        "setup: applied the saved feature choice"
    );
    forget_choice(&config.config_path);
}
