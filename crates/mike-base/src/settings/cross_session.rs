use serde::Serialize;

pub const INPUT_MODE_KEY: &str = "guard.calibration.input_mode";
pub const CAL_MIN_LABELLED_KEY: &str = "guard.calibration.min_labelled";
pub const CAL_WINDOW_DAYS_KEY: &str = "guard.calibration.window_days";
pub const THRESHOLD_MODE_KEY: &str = "guard.threshold.mode";
pub const THRESHOLD_MIN_LABELLED_KEY: &str = "guard.threshold.min_labelled";
pub const HYSTERESIS_KEY: &str = "guard.threshold.hysteresis";
pub const COOLDOWN_DAYS_KEY: &str = "guard.threshold.cooldown_days";
pub const CEILING_KEY: &str = "guard.threshold.ceiling";
pub const ALLOW_LOWERING_FOR_WAKE_KEY: &str = "guard.threshold.allow_lowering_for_wake";
pub const WEIGHT_OPERATOR_KEY: &str = "guard.threshold.weight.operator";
pub const WEIGHT_SESSION_REPLY_KEY: &str = "guard.threshold.weight.session_reply";
pub const WEIGHT_SESSION_END_KEY: &str = "guard.threshold.weight.session_end";
pub const WEIGHT_NEXT_MESSAGE_KEY: &str = "guard.threshold.weight.next_message";
pub const STEERER_BAR_KEY: &str = "guard.threshold.steerer_bar";
pub const PROPOSER_SOURCES_KEY: &str = "directives.proposer.sources";
pub const PROPOSER_WINDOW_DAYS_KEY: &str = "directives.proposer.window_days";
pub const PROPOSER_MIN_SESSIONS_KEY: &str = "directives.proposer.min_sessions";
pub const PROPOSER_MIN_CONFIRMED_KEY: &str = "directives.proposer.min_confirmed";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossSessionSettings {
    pub input_mode: String,
    pub calibration_min_labelled: i64,
    pub calibration_window_days: i64,
    pub threshold_mode: String,
    pub threshold_min_labelled: i64,
    pub hysteresis: f64,
    pub cooldown_days: i64,
    pub ceiling: f64,
    pub allow_lowering_for_wake: bool,
    pub weight_operator: f64,
    pub weight_session_reply: f64,
    pub weight_session_end: f64,
    pub weight_next_message: f64,
    pub steerer_bar: f64,
    pub proposer_sources: Vec<String>,
    pub proposer_window_days: i64,
    pub min_sessions: i64,
    pub min_confirmed: i64,
}

impl Default for CrossSessionSettings {
    fn default() -> Self {
        Self {
            input_mode: "off".to_owned(),
            calibration_min_labelled: 20,
            calibration_window_days: 30,
            threshold_mode: "off".to_owned(),
            threshold_min_labelled: 30,
            hysteresis: 0.05,
            cooldown_days: 7,
            ceiling: 0.95,
            allow_lowering_for_wake: false,
            weight_operator: 1.0,
            weight_session_reply: 1.0,
            weight_session_end: 0.8,
            weight_next_message: 0.5,
            steerer_bar: 0.75,
            proposer_sources: vec![
                "repeated_directive".to_owned(),
                "confirmed_handback".to_owned(),
            ],
            proposer_window_days: 30,
            min_sessions: 2,
            min_confirmed: 3,
        }
    }
}

fn int(v: &serde_json::Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn float(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn text(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn list(v: &serde_json::Value) -> Option<Vec<String>> {
    v.as_array().map(|a| {
        a.iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    })
}

impl CrossSessionSettings {
    pub fn is_ours(key: &str) -> bool {
        key.starts_with("guard.calibration.")
            || key.starts_with("guard.threshold.")
            || key.starts_with("directives.")
    }

    pub fn from_rows<'a>(rows: impl IntoIterator<Item = (&'a str, &'a serde_json::Value)>) -> Self {
        let mut s = Self::default();
        for (key, v) in rows {
            s.apply(key, v);
        }
        s
    }

    fn apply(&mut self, key: &str, v: &serde_json::Value) {
        match key {
            INPUT_MODE_KEY => {
                self.input_mode = text(v)
                    .filter(|m| m == "live")
                    .unwrap_or_else(|| "off".to_owned())
            }
            CAL_MIN_LABELLED_KEY => self.calibration_min_labelled = int(v).unwrap_or(20).max(1),
            CAL_WINDOW_DAYS_KEY => self.calibration_window_days = int(v).unwrap_or(30).max(1),
            THRESHOLD_MODE_KEY => {
                self.threshold_mode = text(v)
                    .filter(|m| m == "apply")
                    .unwrap_or_else(|| "off".to_owned())
            }
            THRESHOLD_MIN_LABELLED_KEY => self.threshold_min_labelled = int(v).unwrap_or(30).max(1),
            HYSTERESIS_KEY => self.hysteresis = float(v).unwrap_or(0.05).clamp(0.0, 0.5),
            COOLDOWN_DAYS_KEY => self.cooldown_days = int(v).unwrap_or(7).max(0),
            CEILING_KEY => self.ceiling = float(v).unwrap_or(0.95).clamp(0.5, 1.0),
            ALLOW_LOWERING_FOR_WAKE_KEY => {
                self.allow_lowering_for_wake = v.as_bool().unwrap_or(false)
            }
            WEIGHT_OPERATOR_KEY => self.weight_operator = float(v).unwrap_or(1.0).clamp(0.0, 1.0),
            WEIGHT_SESSION_REPLY_KEY => {
                self.weight_session_reply = float(v).unwrap_or(1.0).clamp(0.0, 1.0)
            }
            WEIGHT_SESSION_END_KEY => {
                self.weight_session_end = float(v).unwrap_or(0.8).clamp(0.0, 1.0)
            }
            WEIGHT_NEXT_MESSAGE_KEY => {
                self.weight_next_message = float(v).unwrap_or(0.5).clamp(0.0, 1.0)
            }
            STEERER_BAR_KEY => self.steerer_bar = float(v).unwrap_or(0.75).clamp(0.5, 0.99),
            PROPOSER_SOURCES_KEY => self.proposer_sources = list(v).unwrap_or_default(),
            PROPOSER_WINDOW_DAYS_KEY => self.proposer_window_days = int(v).unwrap_or(30).max(1),
            PROPOSER_MIN_SESSIONS_KEY => self.min_sessions = int(v).unwrap_or(2).max(1),
            PROPOSER_MIN_CONFIRMED_KEY => self.min_confirmed = int(v).unwrap_or(3).max(1),
            _ => {}
        }
    }

    pub fn source_weight(&self, source: &str) -> f64 {
        match source {
            "operator" => self.weight_operator,
            "session_reply" => self.weight_session_reply,
            "session_end" => self.weight_session_end,
            "next_message" => self.weight_next_message,
            _ => 0.0,
        }
    }

    pub fn source_enabled(&self, source: &str) -> bool {
        self.proposer_sources.iter().any(|s| s == source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_session_settings_default_off_and_parse() {
        let d = CrossSessionSettings::default();
        assert_eq!(d.input_mode, "off");
        assert_eq!(d.threshold_mode, "off");
        let live = serde_json::json!("live");
        let apply = serde_json::json!("apply");
        let bogus = serde_json::json!("shadow");
        let s = CrossSessionSettings::from_rows([
            (INPUT_MODE_KEY, &live),
            (THRESHOLD_MODE_KEY, &apply),
        ]);
        assert_eq!(
            (s.input_mode.as_str(), s.threshold_mode.as_str()),
            ("live", "apply")
        );
        let s = CrossSessionSettings::from_rows([(INPUT_MODE_KEY, &bogus)]);
        assert_eq!(s.input_mode, "off");
        assert!(d.source_enabled("repeated_directive"));
    }

    #[test]
    fn source_weights_default_weaker_for_automatic_labels_and_clamp() {
        let d = CrossSessionSettings::default();
        assert!(d.source_weight("operator") > d.source_weight("session_end"));
        assert!(d.source_weight("session_end") > d.source_weight("next_message"));
        assert_eq!(d.source_weight("unknown"), 0.0);
        let too_big = serde_json::json!(7.0);
        let s = CrossSessionSettings::from_rows([(WEIGHT_NEXT_MESSAGE_KEY, &too_big)]);
        assert_eq!(s.source_weight("next_message"), 1.0);
    }
}
