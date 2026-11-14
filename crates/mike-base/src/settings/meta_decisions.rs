use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const MODEL_KEY: &str = "decisions.model";
pub const TIMEOUT_MS_KEY: &str = "decisions.timeout_ms";
pub const TURN_GUARD_MODE_KEY: &str = "decisions.turn_guard.mode";
pub const TURN_GUARD_HOLD_BELOW_KEY: &str = "decisions.turn_guard.hold_below";
pub const DEFAULT_MODEL: &str = "typesafe/jev-1.14";
pub const DEFAULT_TIMEOUT_MS: i64 = 3000;
pub const DEFAULT_TURN_GUARD_HOLD_BELOW: f64 = 0.3;
pub const MODES: &[&str] = &["off", "shadow", "on"];
pub const RATE_KEY: &str = "decisions.rate_per_minute";
pub const HOURLY_KEY: &str = "decisions.max_calls_per_hour";
pub const DEFAULT_RATE_PER_MINUTE: i64 = 120;
pub const DEFAULT_MAX_CALLS_PER_HOUR: i64 = 3000;
pub const DAILY_SPEND_KEY: &str = "decisions.daily_spend_ceiling_usd";
pub const DEFAULT_DAILY_SPEND_CEILING_USD: f64 = 5.0;

pub struct PointSetting {
    pub point: &'static str,
    pub mode_key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

macro_rules! points {
    ($(($point:literal, $label:literal, $desc:literal)),* $(,)?) => {
        pub const POINTS: &[PointSetting] = &[$(PointSetting {
            point: $point,
            mode_key: concat!("decisions.", $point, ".mode"),
            label: $label,
            description: $desc,
        }),*];
    };
}

points![
    (
        "turn_judge",
        "Turn judge",
        "Jev answers the turn judge's atomic questions (correct, accept, redirect, abandon, backed by a check) and code composes the label."
    ),
    (
        "correction_classifier",
        "Correction classifier",
        "Jev decides whether the operator's message corrects the previous turn."
    ),
    (
        "session_judge",
        "Session judge",
        "Jev picks the session outcome and scores its quality."
    ),
    (
        "lesson_supply",
        "Lesson supply floor",
        "Jev decides whether a lesson would change what the session does next before it is supplied."
    ),
    (
        "commit_message",
        "Commit message check",
        "Jev checks a session's commit message against its diff and leaves a steering note when they disagree."
    ),
    (
        "rule_check",
        "Rule check per turn",
        "Jev checks each turn against the project's AGENTS.md and CLAUDE.md rules."
    ),
    (
        "verifier_precheck",
        "Verifier pre-check",
        "Jev decides whether a verifier fire is worth sending before it is delivered."
    ),
    (
        "wake_recheck",
        "Wake re-check",
        "Jev re-judges a due wake or capped correction at delivery time."
    ),
    (
        "thread_pick",
        "Thread pick",
        "Jev picks which work thread a prompt continues, or none."
    ),
    (
        "recall_topic",
        "Recall topic",
        "Jev maps a typed recall topic to a catalog topic, or none."
    ),
    (
        "lesson_shape",
        "Lesson shape",
        "Jev decides whether a lesson states a rule a later session can act on."
    ),
    (
        "topic_assign",
        "Topic assigner",
        "Jev picks an existing topic for a new lesson, or new."
    ),
    (
        "lesson_relation",
        "Lesson relations",
        "Jev classifies how two lessons relate."
    ),
    (
        "lesson_relate",
        "Write-time relate",
        "Jev decides how a proposed lesson relates to its nearest existing lessons."
    ),
    (
        "topic_merge",
        "Topic merge",
        "Jev decides whether two topics cover the same area of work."
    ),
    (
        "semantic_tag",
        "Semantic tagger",
        "Jev picks the correction pattern a turn matches, or none."
    ),
    (
        "supply_verdict",
        "Supply verdict",
        "Jev decides whether a supplied memory item was used, corrected anyway or ignored."
    ),
    (
        "search_grade",
        "Search grade",
        "Jev decides whether a search result answers its query."
    ),
    (
        "repeats_group",
        "Repeats grouping",
        "Jev decides whether two requests restate the same ask."
    ),
    (
        "skill_trigger",
        "Skill trigger eval",
        "Jev picks which skill, if any, a message should load."
    ),
    (
        "proposal_order",
        "Proposal queue order",
        "Jev orders the proposal queue by its accept, reject or duplicate guess; it never accepts."
    ),
    (
        "label_prelabel",
        "Label pre-labelling",
        "Jev pre-labels turns so only uncertain ones go to review."
    ),
    (
        "text_origin",
        "Operator or machine text",
        "Jev classifies prompt text the exact markers did not match."
    ),
    (
        "probe_session",
        "Probe session",
        "Jev decides whether a session is a probe."
    ),
    (
        "directive",
        "Directive authorise or forbid",
        "Jev decides whether a sentence authorises, forbids or neither for an action class."
    ),
    (
        "step_class",
        "Step class",
        "Jev picks the action class of a permission sentence."
    ),
    (
        "prod_detect",
        "Prod detection",
        "Jev decides whether an operator message only nudges the session to continue."
    ),
    (
        "ended_on",
        "Ended on",
        "Jev decides what a turn ends on and whether it claims done."
    ),
    (
        "criteria_contract",
        "Criteria contract",
        "Jev decides whether acceptance criteria describe an observable end state."
    ),
    (
        "criteria_source",
        "Criteria source",
        "Jev decides, over all the operator asked a session, which of its linked work threads it was asked to do; only those threads' criteria join its verifier."
    ),
    (
        "steerer_checklist",
        "Steerer checklist",
        "Jev answers one question per criterion before the fleet steerer writes a nudge."
    ),
    (
        "diagnostic_phrase",
        "Diagnostic phrases",
        "Jev decides whether a matched phrase excuses skipped work."
    ),
    (
        "weasel_after",
        "Weasel words after the stream",
        "Jev decides whether a matched weasel word is used as one."
    ),
    (
        "gate_blame",
        "Gate blame",
        "Jev picks the commit or seat a failing gate belongs to."
    ),
    (
        "change_author",
        "Change author",
        "Jev picks the session that made an unattributed change."
    ),
    (
        "blocked_dialog",
        "Blocked dialog",
        "Jev names the dialog a launched session is blocked on."
    ),
    (
        "needs_operator",
        "Needs operator",
        "Jev decides whether a raised question needs the operator."
    ),
    (
        "problem_solver",
        "Problem solver pick",
        "Jev picks the fleet problem solver's action."
    ),
    (
        "leaf_judge",
        "Fleet leaf judge",
        "Jev answers one question per criterion for a fleet leaf."
    ),
    (
        "mail_reply_link",
        "Mail reply linkage",
        "Jev picks which inbox message a reply answers."
    ),
    (
        "project_merge",
        "Project merge suggestion",
        "Jev suggests projects that are the same; it never merges."
    ),
    (
        "thread_status",
        "Thread status suggestion",
        "Jev suggests when a work thread looks finished or parked."
    ),
    (
        "docs_agent",
        "Docs agent",
        "Jev decides which doc categories a change touches."
    ),
    (
        "log_group",
        "Log grouping",
        "Jev decides whether two log lines are the same problem."
    ),
];

pub struct FleetPoint {
    pub point: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const FLEET_POINTS: &[FleetPoint] = &[
    FleetPoint {
        point: "fleet_stop_check",
        label: "Fleet stop check",
        description: "Facts first, then Jev, decide whether a seat's stop goes through or is pushed back once.",
    },
    FleetPoint {
        point: "fleet_wake_check",
        label: "Fleet wake check",
        description: "Jev decides whether a mail or wakeup wakes a seat now or waits for its next turn.",
    },
    FleetPoint {
        point: "fleet_triage",
        label: "Fleet check triage",
        description: "Rules, then Jev, put a retry, fix-code or fix-environment line in a failed check's receipt.",
    },
    FleetPoint {
        point: "fleet_commit_message",
        label: "Fleet commit message",
        description: "Jev warns when a fleet_commit message does not match its diff; it never refuses.",
    },
    FleetPoint {
        point: "fleet_effort",
        label: "Fleet effort split",
        description: "Jev labels request content as work, machinery or waiting where the trigger does not decide it.",
    },
];

pub fn area(point: &str) -> &'static str {
    match point {
        "turn_guard_precheck"
        | "commit_message"
        | "rule_check"
        | "verifier_precheck"
        | "wake_recheck"
        | "step_class"
        | "prod_detect"
        | "ended_on"
        | "criteria_contract"
        | "criteria_source"
        | "steerer_checklist"
        | "diagnostic_phrase"
        | "weasel_after" => "steering",
        "turn_judge"
        | "correction_classifier"
        | "session_judge"
        | "supply_verdict"
        | "label_prelabel" => "judges",
        "lesson_supply" | "thread_pick" | "recall_topic" | "lesson_shape" | "topic_assign"
        | "lesson_relation" | "lesson_relate" | "topic_merge" | "semantic_tag" | "search_grade"
        | "repeats_group" | "skill_trigger" | "proposal_order" => "memory and lessons",
        "text_origin" | "probe_session" | "directive" => "capture",
        p if p.starts_with("fleet_") => "fleet",
        _ => "launch, mail and tools",
    }
}

pub fn point_setting(key: &str) -> Option<&'static PointSetting> {
    POINTS.iter().find(|p| p.mode_key == key)
}

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

fn point_entries() -> impl Iterator<Item = SettingMeta> {
    POINTS.iter().map(|p| SettingMeta {
        kind: SettingKind::Enum,
        default: serde_json::Value::String("on".to_owned()),
        enum_variants: Some(MODES),
        ..base(p.mode_key, p.label, p.description)
    })
}

pub fn entries() -> Vec<SettingMeta> {
    let mut out = fixed_entries();
    out.extend(point_entries());
    out
}

fn fixed_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            kind: SettingKind::Int,
            default: serde_json::Value::from(DEFAULT_RATE_PER_MINUTE),
            min_i64: Some(1),
            max_i64: Some(10_000),
            ..base(
                RATE_KEY,
                "Decisions per minute",
                "Decisions have their own limiter, apart from the shared LLM limiter; past this rate a decision behaves as if decisions were off.",
            )
        },
        SettingMeta {
            kind: SettingKind::Int,
            default: serde_json::Value::from(DEFAULT_MAX_CALLS_PER_HOUR),
            min_i64: Some(1),
            max_i64: Some(200_000),
            ..base(
                HOURLY_KEY,
                "Decisions per hour",
                "The hourly ceiling of the decisions limiter.",
            )
        },
        SettingMeta {
            kind: SettingKind::Float,
            default: serde_json::Value::from(DEFAULT_DAILY_SPEND_CEILING_USD),
            min_f64: Some(0.0),
            max_f64: Some(1_000.0),
            ..base(
                DAILY_SPEND_KEY,
                "Decisions daily spend ceiling (USD)",
                "Once Jev calls have cost this much today (UTC), every decision point takes its off path and the call is recorded as refused_spend. 0 turns the ceiling off.",
            )
        },
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
