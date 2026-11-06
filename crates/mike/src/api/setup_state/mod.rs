use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::claude_code::{Locations, gather_status};
use crate::config::AppState;
use crate::doctor::{Report, build_report};
use crate::feature_presets::{LESSONS_SETTING, Preset, plan};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub(crate) struct ClaudeCodeState {
    pub(crate) route: Option<&'static str>,
    pub(crate) function_name: Option<String>,
    pub(crate) auto_trust: Option<bool>,
    pub(crate) mods_in_place: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct PresetToggle {
    pub(crate) key: &'static str,
    pub(crate) enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeaturePreset {
    pub(crate) id: &'static str,
    pub(crate) toggles: Vec<PresetToggle>,
    pub(crate) lessons_setting: &'static str,
    pub(crate) session_lessons: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SetupState {
    pub(crate) doctor: Report,
    pub(crate) claude_code: Option<ClaudeCodeState>,
    pub(crate) feature_presets: Vec<FeaturePreset>,
}

pub(crate) fn feature_presets() -> Vec<FeaturePreset> {
    [Preset::WatchOnly, Preset::Recommended, Preset::Everything]
        .into_iter()
        .map(|preset| {
            let wanted = plan(preset);
            FeaturePreset {
                id: preset.as_str(),
                toggles: wanted
                    .toggles
                    .into_iter()
                    .map(|(key, enabled)| PresetToggle { key, enabled })
                    .collect(),
                lessons_setting: LESSONS_SETTING,
                session_lessons: wanted.session_lessons,
            }
        })
        .collect()
}

fn claude_code_state() -> Option<ClaudeCodeState> {
    let locations = Locations::from_environment().ok()?;
    let status = gather_status(&locations);
    Some(ClaudeCodeState {
        route: status.route.map(|route| route.as_str()),
        mods_in_place: status.mods_in_place(),
        function_name: status.function_name,
        auto_trust: status.auto_trust,
    })
}

pub(crate) async fn get_setup_state(State(state): State<AppState>) -> Json<SetupState> {
    Json(SetupState {
        doctor: build_report(&state.config).await,
        claude_code: claude_code_state(),
        feature_presets: feature_presets(),
    })
}
