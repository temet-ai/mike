use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const MAX_CALLS_PER_DAY_KEY: &str = "steerer.turn_guard.max_calls_per_day";
pub const MAX_LABEL_CALLS_PER_DAY_KEY: &str = "steerer.turn_guard.max_label_calls_per_day";
pub const RECALL_SAMPLE_PCT_KEY: &str = "steerer.turn_guard.recall_sample_pct";
pub const HOLDBACK_PCT_KEY: &str = "steerer.turn_guard.holdback_pct";
pub const LABEL_LIVE_PASSES_KEY: &str = "steerer.turn_guard.label_live_passes";
pub const WAKE_GRACE_RELAPSE_KEY: &str = "steerer.turn_guard.wake_grace_relapse_secs";
pub const WAKE_GRACE_WAIT_KEY: &str = "steerer.turn_guard.wake_grace_wait_secs";
pub const WAKE_CAP_KEY: &str = "steerer.turn_guard.wake_cap_per_session_hour";
pub const AUTO_DEMOTE_KEY: &str = "steerer.turn_guard.auto_demote";
pub const WRONG_WAKE_CEILING_KEY: &str = "steerer.turn_guard.wrong_wake_ceiling_per_week";
pub const FLEET_SEATS_KEY: &str = "steerer.turn_guard.fleet_seats";

fn int_meta(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: i64,
    bounds: (i64, i64),
) -> SettingMeta {
    SettingMeta {
        key,
        group: "Steering: turn guard",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::from(default),
        label,
        description,
        min_i64: Some(bounds.0),
        max_i64: Some(bounds.1),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn auto_demote_meta() -> SettingMeta {
    SettingMeta {
        kind: SettingKind::Bool,
        default: serde_json::Value::Bool(true),
        min_i64: None,
        max_i64: None,
        ..int_meta(
            AUTO_DEMOTE_KEY,
            "Demote live guard questions automatically",
            "An hourly check moves a live question back to shadow when its precision falls below \
             its bar or the holdback arm measures harm, and a wake question to append after two \
             weeks over the wrong-wake ceiling. Demotion only reduces traffic changes. Applies \
             within a minute.",
            0,
            (0, 1),
        )
    }
}

fn label_live_passes_meta() -> SettingMeta {
    SettingMeta {
        kind: SettingKind::Bool,
        default: serde_json::Value::Bool(false),
        min_i64: None,
        max_i64: None,
        ..int_meta(
            LABEL_LIVE_PASSES_KEY,
            "Label live pass rows",
            "The labeller also labels live evaluations that passed (mode live, status \
             resolved_pass, no arm) from the operator's next message, not only shadow and \
             holdback rows. A treated fire is never labelled this way — its next message is \
             confounded by the nudge. Only acts while the turn_guard toggle is on.",
            0,
            (0, 1),
        )
    }
}

fn fleet_seats_meta() -> SettingMeta {
    SettingMeta {
        kind: SettingKind::Bool,
        default: serde_json::Value::Bool(false),
        min_i64: None,
        max_i64: None,
        ..int_meta(
            FLEET_SEATS_KEY,
            "Judge fleet seats' done claims and waits",
            "The turn guard's unobserved done-claim and unbacked-wait questions also judge fleet \
             seats, in append mode behind holdback. Relapse and authorised handback stay off for \
             fleet seats. Only acts while the turn_guard toggle is on.",
            0,
            (0, 1),
        )
    }
}

fn jc17_entries() -> Vec<SettingMeta> {
    vec![
        int_meta(
            HOLDBACK_PCT_KEY,
            "Holdback (%)",
            "The share of a live question's would-fires withheld at random as the control arm \
             (operator decision D8). 0 turns causal measurement off. Applies within a minute.",
            10,
            (0, 50),
        ),
        int_meta(
            WAKE_GRACE_RELAPSE_KEY,
            "Relapse wake grace (seconds)",
            "How long after the turn ends a relapse wake waits before a push, so an operator \
             who is typing is not interrupted (D6).",
            30,
            (0, 600),
        ),
        int_meta(
            WAKE_GRACE_WAIT_KEY,
            "Unbacked-wait wake grace (seconds)",
            "How long after the turn ends an unbacked-wait wake waits before a push (D7).",
            90,
            (0, 900),
        ),
        int_meta(
            WAKE_CAP_KEY,
            "Guard wakes per session per hour",
            "The most guard wakes one session gets in a rolling hour; beyond it the nudge stays \
             queued for the next request (D7).",
            3,
            (0, 20),
        ),
        auto_demote_meta(),
        int_meta(
            WRONG_WAKE_CEILING_KEY,
            "Wrong wakes per week",
            "Estimated wrong wakes a week above which a wake question is demoted to append, after \
             two weeks running.",
            2,
            (0, 50),
        ),
    ]
}

pub(super) fn entries() -> Vec<SettingMeta> {
    let mut entries = base_entries();
    entries.extend(jc17_entries());
    entries
}

fn base_entries() -> Vec<SettingMeta> {
    vec![
        int_meta(
            MAX_CALLS_PER_DAY_KEY,
            "Turn guard calls per day",
            "The most model calls the turn guard makes in a UTC day, its quorum calls included. \
             At the cap the open questions are recorded as daily_cap and left out of recall. Only \
             acts while the turn_guard toggle is on.",
            150,
            (0, 1000),
        ),
        int_meta(
            MAX_LABEL_CALLS_PER_DAY_KEY,
            "Turn guard labelling calls per day",
            "The most calls the guard's labeller makes in a UTC day to label shadow rows from the \
             operator's next message. Only acts while the turn_guard toggle is on.",
            80,
            (0, 1000),
        ),
        int_meta(
            RECALL_SAMPLE_PCT_KEY,
            "Turn guard recall sample (%)",
            "The share of gate-closed operator turn ends the labeller also labels, so recall has a \
             denominator. Changes cost, not behaviour.",
            10,
            (0, 100),
        ),
        label_live_passes_meta(),
        fleet_seats_meta(),
    ]
}
