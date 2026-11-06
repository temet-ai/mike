use clap::ValueEnum;

use crate::feature_toggles::{FeatureToggles, TOGGLE_META, get_toggle_enabled};

pub(crate) const RECOMMENDED_ON: [&str; 3] = ["recall_injector", "agent_lessons", "reminders"];
pub(crate) const LESSONS_SETTING: &str = "reminders.inject_lessons";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Preset {
    WatchOnly,
    Recommended,
    Everything,
}

impl Preset {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Preset::WatchOnly => "watch-only",
            Preset::Recommended => "recommended",
            Preset::Everything => "everything",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "watch-only" | "watch_only" => Some(Preset::WatchOnly),
            "recommended" => Some(Preset::Recommended),
            "everything" => Some(Preset::Everything),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) toggles: Vec<(&'static str, bool)>,
    pub(crate) session_lessons: bool,
}

pub(crate) fn plan(preset: Preset) -> Plan {
    let defaults = FeatureToggles::default();
    let toggles = TOGGLE_META
        .iter()
        .map(|meta| {
            let default = get_toggle_enabled(&defaults, meta.key).unwrap_or(false);
            let wanted = match preset {
                Preset::Everything => true,
                Preset::WatchOnly => default,
                Preset::Recommended => default || RECOMMENDED_ON.contains(&meta.key),
            };
            (meta.key, wanted)
        })
        .collect();
    Plan {
        toggles,
        session_lessons: preset != Preset::WatchOnly,
    }
}

pub(crate) fn default_off_keys() -> Vec<&'static str> {
    let defaults = FeatureToggles::default();
    TOGGLE_META
        .iter()
        .filter(|meta| !get_toggle_enabled(&defaults, meta.key).unwrap_or(false))
        .map(|meta| meta.key)
        .collect()
}
