use super::*;

#[test]
fn judge_model_keys_map_to_their_live_pointer() {
    assert_eq!(
        pointer_key_for_model_key("judge_worker.model"),
        Some("turn_judge.live_bundle_id")
    );
    assert_eq!(
        pointer_key_for_model_key("session_judge.model"),
        Some("session_judge.live_bundle_id")
    );
}

#[test]
fn other_keys_have_no_pointer() {
    assert_eq!(pointer_key_for_model_key("steerer.model"), None);
    assert_eq!(pointer_key_for_model_key("judge_lessons.model"), None);
}

#[test]
fn pointer_keys_are_recognised() {
    assert!(is_pointer_key("turn_judge.live_bundle_id"));
    assert!(is_pointer_key("session_judge.live_bundle_id"));
    assert!(!is_pointer_key("judge_worker.model"));
}

#[test]
fn pointer_value_reads_the_matching_overlay_field() {
    let overlay = super::super::SettingsOverlay {
        turn_judge_live_bundle_id: 67,
        session_judge_live_bundle_id: 66,
        ..Default::default()
    };
    assert_eq!(pointer_value(&overlay, "turn_judge.live_bundle_id"), 67);
    assert_eq!(pointer_value(&overlay, "session_judge.live_bundle_id"), 66);
}
