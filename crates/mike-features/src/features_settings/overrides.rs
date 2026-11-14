use serde_json::Value;

use super::sections::{RemindersConfig, SessionJudgeConfig, WeaselInterceptConfig};
use crate::config::ConfigSections;

pub fn apply_override(sections: &mut ConfigSections, key: &str, value: &Value) -> bool {
    let Some(field) = key.strip_prefix("session_judge.") else {
        return false;
    };
    let mut handled = true;
    sections.update::<SessionJudgeConfig>(|cfg| match field {
        "model" => {
            if let Some(v) = value.as_str()
                && !v.is_empty()
            {
                cfg.model = v.to_owned();
            }
        }
        "system_prompt" => {
            if let Some(v) = value.as_str() {
                cfg.system_prompt = v.to_owned();
            }
        }
        "batch_size" => {
            if let Some(v) = value.as_i64() {
                cfg.batch_size = v;
            }
        }
        "min_session_age_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.min_session_age_secs = v;
            }
        }
        "max_turns_in_prompt" => {
            if let Some(v) = value.as_u64() {
                cfg.max_turns_in_prompt = v as usize;
            }
        }
        "poll_interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.poll_interval_secs = v;
            }
        }
        "request_timeout_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.request_timeout_secs = v;
            }
        }
        _ => handled = false,
    });
    handled
}

pub fn boot_toggles(sections: &ConfigSections) -> Vec<(&'static str, bool)> {
    vec![
        ("reminders", sections.get::<RemindersConfig>().enabled),
        (
            "weasel_intercept",
            sections.get::<WeaselInterceptConfig>().enabled,
        ),
    ]
}
