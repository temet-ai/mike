use super::*;
use crate::store::persistence::llm_role_health::WorkerToggleState;
use chrono::{DateTime, Duration, Utc};

const OFF_GRACE_HOURS: i64 = 48;
const LESSON_TOGGLE: &str = "judge_lessons";
const SETTINGS_PAGE: &str = "/settings/steering";

const WORKER_TOGGLES: [(&str, &str); 4] = [
    ("steerer", "Steerers"),
    ("judge_worker", "Turn judge"),
    ("session_judge", "Session judge"),
    (LESSON_TOGGLE, "Lesson proposer"),
];

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: "worker_toggles",
        label: "LLM worker switches",
        state,
        detail,
        fix,
    }
}

fn label_for(key: &str) -> &'static str {
    WORKER_TOGGLES
        .iter()
        .find(|(known, _)| *known == key)
        .map_or("Worker", |(_, label)| label)
}

fn long_off(states: &[WorkerToggleState], now: DateTime<Utc>) -> Vec<&WorkerToggleState> {
    states
        .iter()
        .filter(|s| !s.enabled && now - s.updated_at >= Duration::hours(OFF_GRACE_HOURS))
        .collect()
}

fn off_sentence(state: &WorkerToggleState, now: DateTime<Utc>, corrections: Option<i64>) -> String {
    let days = (now - state.updated_at).num_days();
    let since = state.updated_at.format("%Y-%m-%d %H:%M UTC");
    let mut sentence = format!(
        "{} ({}) has been switched off for {days} day(s), since {since}.",
        label_for(&state.key),
        state.key
    );
    if let Some(count) = corrections.filter(|c| *c > 0 && state.key == LESSON_TOGGLE) {
        sentence.push_str(&format!(
            " {count} corrected operator turn(s) have been recorded since and none of them can become a lesson candidate while it is off."
        ));
    }
    sentence
}

pub(crate) fn worker_toggles_row(
    now: DateTime<Utc>,
    states: &[WorkerToggleState],
    corrections: Option<i64>,
) -> CapabilityRow {
    let off = long_off(states, now);
    if off.is_empty() {
        return row(
            CapabilityState::Active,
            "No LLM worker switch has been off for more than two days".to_owned(),
            String::new(),
        );
    }
    let detail = off
        .iter()
        .map(|s| off_sentence(s, now, corrections))
        .collect::<Vec<_>>()
        .join(" ");
    row(
        CapabilityState::Degraded,
        detail,
        format!(
            "Switch the worker back on at {SETTINGS_PAGE} once its model works, or leave it off on purpose and dismiss this notice."
        ),
    )
}

pub(crate) async fn worker_toggles_capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is disabled, so switch changes are not dated".to_owned(),
            String::new(),
        );
    };
    let keys: Vec<String> = WORKER_TOGGLES
        .iter()
        .map(|(key, _)| (*key).to_owned())
        .collect();
    let states = match persist.worker_toggle_states(&keys).await {
        Ok(states) => states,
        Err(e) => {
            return row(
                CapabilityState::Degraded,
                format!("Could not read the worker switches: {e}"),
                "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
            );
        }
    };
    let lessons_off_since = long_off(&states, Utc::now())
        .into_iter()
        .find(|s| s.key == LESSON_TOGGLE)
        .map(|s| s.updated_at);
    let corrections = match lessons_off_since {
        Some(since) => persist.corrected_turns_since(since).await.ok(),
        None => None,
    };
    worker_toggles_row(Utc::now(), &states, corrections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .single()
            .unwrap_or_else(Utc::now)
    }

    fn toggle(key: &str, enabled: bool, hours_ago: i64) -> WorkerToggleState {
        WorkerToggleState {
            key: key.to_owned(),
            enabled,
            updated_at: now() - Duration::hours(hours_ago),
        }
    }

    #[test]
    fn row_lesson_toggle_off_for_nine_days_is_degraded_with_the_correction_count() {
        let states = [toggle("judge_lessons", false, 9 * 24 + 16)];
        let r = worker_toggles_row(now(), &states, Some(354));
        assert_eq!(r.state, CapabilityState::Degraded);
        assert!(r.detail.contains("Lesson proposer (judge_lessons)"));
        assert!(r.detail.contains("off for 9 day(s)"), "{}", r.detail);
        assert!(r.detail.contains("354 corrected operator turn(s)"));
        assert!(r.fix.contains(SETTINGS_PAGE));
    }

    #[test]
    fn row_toggle_off_for_less_than_two_days_is_active() {
        let states = [toggle("judge_worker", false, 20)];
        assert_eq!(
            worker_toggles_row(now(), &states, None).state,
            CapabilityState::Active
        );
    }

    #[test]
    fn row_switched_on_toggles_are_active() {
        let states = [
            toggle("judge_lessons", true, 400),
            toggle("steerer", true, 400),
        ];
        let r = worker_toggles_row(now(), &states, Some(10));
        assert_eq!(r.state, CapabilityState::Active);
        assert!(r.fix.is_empty());
    }

    #[test]
    fn row_names_every_long_off_worker() {
        let states = [
            toggle("steerer", false, 100),
            toggle("session_judge", false, 100),
        ];
        let r = worker_toggles_row(now(), &states, None);
        assert!(r.detail.contains("Steerers (steerer)"), "{}", r.detail);
        assert!(
            r.detail.contains("Session judge (session_judge)"),
            "{}",
            r.detail
        );
        assert!(
            !r.detail.contains("corrected operator turn"),
            "{}",
            r.detail
        );
    }
}
