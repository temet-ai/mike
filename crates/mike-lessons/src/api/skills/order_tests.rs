use serde_json::json;

use super::*;

fn row(draft_id: i64, status: &str, review: Option<&str>) -> SkillFolderRow {
    SkillFolderRow {
        folder: format!("skill-{draft_id}"),
        draft_id: Some(draft_id),
        status: Some(status.to_owned()),
        review: review.map(str::to_owned),
        revision: Some(1),
        lesson_watermark: Some(10),
        lesson_count: Some(3),
        description: Some("when to use it".to_owned()),
        drafted_at: None,
        evaluated_at: None,
        edited_at: None,
        trigger_score: Some(0.9),
        patched_trigger_score: None,
        reviewer: None,
        verdict_reasons: None,
        approved_draft_id: None,
        first_written: None,
        live: false,
        installs: json!([]),
        retired_by_user_at: None,
        live_use: None,
    }
}

#[test]
fn waiting_keeps_only_drafts_the_operator_still_has_to_review() {
    let rows = [
        row(1, "accepted", None),
        row(2, "accepted", Some("approved")),
        row(3, "draft", None),
    ];
    let ids: Vec<i64> = waiting(&rows, Mode::Ask)
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(ids, vec![1]);
    assert!(waiting(&rows, Mode::Auto).is_empty());
}

#[test]
fn order_by_suggestion_puts_likely_approvals_first_and_other_folders_last() {
    let mut rows = vec![
        row(1, "accepted", None),
        row(2, "accepted", Some("approved")),
        row(3, "accepted", None),
        row(4, "accepted", None),
    ];
    let suggested = BTreeMap::from([(3, 0.9), (1, 0.2)]);
    order_by_suggestion(&mut rows, &suggested, Mode::Ask);
    assert_eq!(
        rows.iter().filter_map(|r| r.draft_id).collect::<Vec<_>>(),
        vec![3, 1, 4, 2]
    );
}

#[test]
fn candidate_state_describes_the_skill_draft() {
    let built = candidate(5, &row(5, "accepted", None));
    assert_eq!(built.key, "skill_draft:5");
    assert_eq!(built.question_key, "skill_draft_5");
    assert_eq!(built.item["skill_folder"], "skill-5");
    assert_eq!(built.item["lessons_in_draft"], 3);
}

#[test]
fn cached_is_empty_without_a_background_guess() {
    let rows = [row(900_001, "accepted", None)];
    assert!(cached(&rows, Mode::Ask).is_empty());
}
