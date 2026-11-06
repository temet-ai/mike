#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmingState {
    Calm,
    Armed,
    Suppressed,
}

pub(crate) fn derive_arming_state(
    recent_outcomes_newest_first: &[&str],
    arming_window_turns: usize,
    closed_turn_count: i64,
    suppress_first_n_turns: i64,
    has_done_claim: bool,
) -> ArmingState {
    let decisive_within_window = recent_outcomes_newest_first
        .iter()
        .take(arming_window_turns)
        .find(|code| matches!(**code, "accepted" | "corrected"));

    if decisive_within_window == Some(&"corrected") {
        return ArmingState::Armed;
    }

    if closed_turn_count < suppress_first_n_turns && !has_done_claim {
        return ArmingState::Suppressed;
    }

    ArmingState::Calm
}

pub(crate) fn has_explicit_done_claim(text: &str) -> bool {
    const DONE_CLAIM_MARKERS: &[&str] = &[
        "all tests pass",
        "tests are passing",
        "done.",
        "done!",
        "finished",
        "completed",
        "should be good now",
        "should be fixed now",
        "ready for review",
        "fixed the",
        "resolved the",
        "this is complete",
        "the fix is",
        "i've verified",
        "i have verified",
    ];
    let lower = text.to_lowercase();
    DONE_CLAIM_MARKERS.iter().any(|m| lower.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calm_with_no_history() {
        assert_eq!(derive_arming_state(&[], 3, 10, 3, false), ArmingState::Calm);
    }

    #[test]
    fn armed_immediately_after_correction() {
        assert_eq!(
            derive_arming_state(&["corrected"], 3, 10, 3, false),
            ArmingState::Armed
        );
    }

    #[test]
    fn armed_within_window_but_not_beyond_it() {
        let outcomes = ["unknown", "unknown", "corrected", "unknown"];
        assert_eq!(
            derive_arming_state(&outcomes, 3, 10, 3, false),
            ArmingState::Armed
        );
        assert_eq!(
            derive_arming_state(&outcomes, 2, 10, 3, false),
            ArmingState::Calm
        );
    }

    #[test]
    fn accepted_hard_resets_even_with_older_correction_in_window() {
        let outcomes = ["accepted", "corrected"];
        assert_eq!(
            derive_arming_state(&outcomes, 3, 10, 3, false),
            ArmingState::Calm
        );
    }

    #[test]
    fn corrected_more_recent_than_accepted_still_arms() {
        let outcomes = ["corrected", "accepted"];
        assert_eq!(
            derive_arming_state(&outcomes, 3, 10, 3, false),
            ArmingState::Armed
        );
    }

    #[test]
    fn suppressed_in_early_session_without_done_claim() {
        assert_eq!(
            derive_arming_state(&[], 3, 1, 3, false),
            ArmingState::Suppressed
        );
    }

    #[test]
    fn suppression_lifts_on_explicit_done_claim() {
        assert_eq!(derive_arming_state(&[], 3, 1, 3, true), ArmingState::Calm);
    }

    #[test]
    fn suppression_lifts_once_past_the_threshold() {
        assert_eq!(derive_arming_state(&[], 3, 3, 3, false), ArmingState::Calm);
    }

    #[test]
    fn arming_takes_priority_over_suppression() {
        assert_eq!(
            derive_arming_state(&["corrected"], 3, 1, 3, false),
            ArmingState::Armed
        );
    }

    #[test]
    fn skipped_outcomes_still_count_toward_the_window() {
        let outcomes = ["unknown", "unknown", "unknown", "corrected"];
        assert_eq!(
            derive_arming_state(&outcomes, 3, 10, 3, false),
            ArmingState::Calm
        );
    }

    #[test]
    fn done_claim_detects_common_completion_phrasing() {
        assert!(has_explicit_done_claim("All tests pass, this is complete."));
        assert!(has_explicit_done_claim("Fixed the bug, ready for review."));
        assert!(!has_explicit_done_claim("still working on the refactor"));
        assert!(!has_explicit_done_claim(""));
    }
}
