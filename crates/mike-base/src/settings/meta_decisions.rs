use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const MODEL_KEY: &str = "decisions.model";
pub const TIMEOUT_MS_KEY: &str = "decisions.timeout_ms";
pub const TURN_GUARD_MODE_KEY: &str = "decisions.turn_guard.mode";
pub const TURN_GUARD_HOLD_BELOW_KEY: &str = "decisions.turn_guard.hold_below";
pub const DEFAULT_MODEL: &str = "typesafe/jev-1.14";
pub const DEFAULT_TIMEOUT_MS: i64 = 3000;
pub const DEFAULT_TURN_GUARD_HOLD_BELOW: f64 = 0.5;
pub const MODES: &[&str] = &["off", "shadow", "on"];

fn base(key: &'static str, label: &'static str, description: &'static str) -> SettingMeta {
    SettingMeta {
        key,
        group: "Decisions",
        kind: SettingKind::String,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Null,
        label,
        description,
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

pub fn entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            default: serde_json::Value::String(DEFAULT_MODEL.to_owned()),
            max_len: Some(120),
            ..base(
                MODEL_KEY,
                "Decisions model",
                "The Jev model asked at every decision point, on OpenRouter's Decisions endpoint.",
            )
        },
        SettingMeta {
            kind: SettingKind::Int,
            default: serde_json::Value::from(DEFAULT_TIMEOUT_MS),
            min_i64: Some(200),
            max_i64: Some(30_000),
            ..base(
                TIMEOUT_MS_KEY,
                "Decision timeout (ms)",
                "How long a decision waits for Jev; past it the point behaves as if decisions were off.",
            )
        },
        SettingMeta {
            kind: SettingKind::Enum,
            default: serde_json::Value::String("on".to_owned()),
            enum_variants: Some(MODES),
            ..base(
                TURN_GUARD_MODE_KEY,
                "Turn-guard pre-check",
                "On holds the turn-guard fires Jev's answers say not to send; shadow only records the answer, for debugging.",
            )
        },
        SettingMeta {
            kind: SettingKind::Float,
            default: serde_json::Value::from(DEFAULT_TURN_GUARD_HOLD_BELOW),
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            ..base(
                TURN_GUARD_HOLD_BELOW_KEY,
                "Turn-guard hold threshold",
                "A fire whose combined pre-check score is below this is held (on) or marked would-hold (shadow).",
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_settings_defaults_match_the_overlay_and_rows_apply() {
        let default = crate::settings::SettingsOverlay::default();
        assert_eq!(default.decisions_model, DEFAULT_MODEL);
        assert_eq!(default.decisions_timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(default.decisions_turn_guard_mode, "on");
        assert_eq!(default.llm_budget_decision_share_pct, 5);
        let overlay = crate::settings::apply_db_rows_to_overlay(vec![
            (
                MODEL_KEY.to_owned(),
                serde_json::json!("~typesafe/jev-latest"),
            ),
            (TIMEOUT_MS_KEY.to_owned(), serde_json::json!(900)),
            (TURN_GUARD_MODE_KEY.to_owned(), serde_json::json!("shadow")),
            (
                TURN_GUARD_HOLD_BELOW_KEY.to_owned(),
                serde_json::json!(0.25),
            ),
            (
                "llm.budget.decision.share_pct".to_owned(),
                serde_json::json!(8),
            ),
        ]);
        assert_eq!(overlay.decisions_model, "~typesafe/jev-latest");
        assert_eq!(overlay.decisions_timeout_ms, 900);
        assert_eq!(overlay.decisions_turn_guard_mode, "shadow");
        assert!((overlay.decisions_turn_guard_hold_below - 0.25).abs() < 1e-9);
        assert_eq!(overlay.llm_budget_decision_share_pct, 8);
    }

    #[test]
    fn decisions_mode_row_outside_the_enum_keeps_the_default() {
        let overlay = crate::settings::apply_db_rows_to_overlay(vec![(
            TURN_GUARD_MODE_KEY.to_owned(),
            serde_json::json!("sometimes"),
        )]);
        assert_eq!(overlay.decisions_turn_guard_mode, "on");
    }

    #[test]
    fn every_decisions_entry_is_registered() {
        for entry in entries() {
            assert!(
                crate::settings::meta::SETTINGS_META
                    .iter()
                    .any(|m| m.key == entry.key),
                "{} is not in SETTINGS_META",
                entry.key
            );
        }
    }
}
