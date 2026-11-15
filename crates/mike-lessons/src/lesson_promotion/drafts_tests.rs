use super::super::test_support::{draft, key, lesson, project_uuid};
use super::*;

fn placed(id: i64, topic: &str, placement: Option<&str>, covered_by: Option<&str>) -> LessonRow {
    LessonRow {
        placement: placement.map(str::to_owned),
        covered_by: covered_by.map(str::to_owned),
        ..lesson(id, Some(topic))
    }
}

#[test]
fn the_newest_reviewed_draft_ships_in_auto_mode_and_only_the_approved_one_in_ask_mode() {
    let accepted = draft(5, None, 2, "a");
    let approved = draft(6, Some("approved"), 1, "b");
    let chosen = chosen_draft(true, Some(&accepted), Some(&approved), None);
    assert_eq!(chosen.map(|found| found.id), Some(5));
    let accepted = draft(5, None, 1, "a");
    let approved = draft(6, Some("approved"), 2, "b");
    let chosen = chosen_draft(true, Some(&accepted), Some(&approved), None);
    assert_eq!(chosen.map(|found| found.id), Some(6));
    assert!(chosen_draft(false, Some(&accepted), None, None).is_none());
    assert_eq!(
        chosen_draft(false, Some(&accepted), Some(&approved), None).map(|found| found.id),
        Some(6)
    );
}

#[test]
fn a_rejection_never_brings_back_an_earlier_revision() {
    let older = draft(5, None, 1, "a");
    let newest = draft(6, Some("rejected"), 2, "b");
    assert!(chosen_draft(true, Some(&older), None, Some(&newest)).is_none());
    let later = draft(7, None, 3, "c");
    assert_eq!(
        chosen_draft(true, Some(&later), None, Some(&newest)).map(|found| found.id),
        Some(7)
    );
    let approved = draft(4, Some("approved"), 1, "d");
    assert_eq!(
        chosen_draft(true, Some(&older), Some(&approved), Some(&newest)).map(|found| found.id),
        Some(4)
    );
}

#[test]
fn an_accepted_draft_keeps_shipping_after_its_topic_gains_a_lesson() {
    let accepted = draft(5, None, 2, "a");
    assert_eq!(
        chosen_draft(true, Some(&accepted), None, None).map(|found| found.id),
        Some(5)
    );
}

#[test]
fn only_lessons_placed_as_skills_or_unplaced_or_covered_by_their_own_skill_feed_a_skill() {
    assert!(feeds_topic_skill(
        &placed(1, "gate", Some("skill"), None),
        "gate"
    ));
    assert!(feeds_topic_skill(&placed(2, "gate", None, None), "gate"));
    for elsewhere in ["memory", "rule", "agent"] {
        assert!(!feeds_topic_skill(
            &placed(3, "gate", Some(elsewhere), None),
            "gate"
        ));
    }
    let nowhere = |covered: &str| placed(4, "gate", Some("nowhere"), Some(covered));
    assert!(feeds_topic_skill(&nowhere("gate/SKILL.md"), "gate"));
    assert!(!feeds_topic_skill(&nowhere("gate/SKILL.md"), "lint"));
    assert!(!feeds_topic_skill(&nowhere("docs-style.md"), "docs"));
    assert!(!feeds_topic_skill(
        &placed(5, "gate", Some("nowhere"), None),
        "gate"
    ));
}

#[test]
fn draft_references_skip_the_lessons_name_bad_names_and_entries_without_content() {
    let mut found = draft(5, None, 2, "a");
    found.reference_files = serde_json::json!([
        {"name": "restarting", "when_to_read": "you restart.", "content": "# Restarting\n"},
        {"name": "lessons", "when_to_read": "x", "content": "overwrite"},
        {"name": "../escape", "when_to_read": "x", "content": "escape"},
        {"name": "no-content", "when_to_read": "x"},
    ]);
    assert_eq!(
        drafted_references(&found),
        vec![("restarting".to_owned(), "# Restarting\n".to_owned())]
    );
    found.reference_files = serde_json::Value::String(
        r##"[{"name": "restarting", "content": "# Restarting\n"}]"##.to_owned(),
    );
    assert_eq!(drafted_references(&found).len(), 1);
    found.reference_files = serde_json::Value::Null;
    assert!(drafted_references(&found).is_empty());
}

#[test]
fn draft_sets_keep_the_highest_watermark_per_folder_and_review() {
    let sets = DraftSets::from_rows(vec![
        draft(9, None, 3, "newest"),
        draft(8, None, 2, "older"),
        draft(7, Some("approved"), 1, "approved"),
        draft(6, Some("rejected"), 4, "rejected"),
    ]);
    let wanted = key("deploy-and-restart");
    assert_eq!(sets.accepted[&wanted].id, 9);
    assert_eq!(sets.approved[&wanted].id, 7);
    assert_eq!(sets.rejected[&wanted].id, 6);
}

#[test]
fn a_reviewed_skill_keeps_the_lessons_it_was_drafted_from() {
    let lessons = vec![
        placed(1, "deploy-and-restart", Some("memory"), None),
        placed(2, "deploy-and-restart", Some("rule"), None),
        placed(3, "unreviewed", Some("memory"), None),
    ];
    let mut reviewed = draft(5, None, 2, "reviewed");
    reviewed.lesson_ids = vec![1, 2];
    let sets = DraftSets::from_rows(vec![reviewed]);
    let template = TopicCandidate {
        project: "/work/proj".to_owned(),
        folder: String::new(),
        rows: Vec::new(),
        roots: vec!["/elsewhere".to_owned()],
    };
    let mut candidates = BTreeMap::new();
    keep_reviewed(
        &project_uuid().to_string(),
        &lessons,
        &sets,
        &mut candidates,
        &template,
    );
    let kept = &candidates[&key("deploy-and-restart")];
    assert_eq!(kept.folder, "deploy-and-restart");
    assert_eq!(
        kept.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(kept.roots, vec!["/elsewhere".to_owned()]);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn a_draft_of_another_project_is_not_kept_here() {
    let sets = DraftSets::from_rows(vec![draft(5, None, 2, "reviewed")]);
    let template = TopicCandidate {
        project: "/work/proj".to_owned(),
        folder: String::new(),
        rows: Vec::new(),
        roots: Vec::new(),
    };
    let mut candidates = BTreeMap::new();
    keep_reviewed(
        "11111111-0000-4000-8000-000000000000",
        &[lesson(1, None)],
        &sets,
        &mut candidates,
        &template,
    );
    assert!(candidates.is_empty());
}
