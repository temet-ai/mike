use super::select::PromptLessons;
use super::*;
use crate::lesson_worker::{LessonRoute, LessonSnapshot};

#[test]
fn arm_is_stable_for_a_session() {
    let id = "5d1c3b5e-0d3c-4a4e-9a55-3f2a0c0de001";
    assert_eq!(arm_for(id), arm_for(id));
}

#[test]
fn about_a_fifth_of_sessions_are_control() {
    let control = (0..2000)
        .filter(|n| arm_for(&format!("session-{n}")) == Arm::Control)
        .count();
    assert!(
        (300..=500).contains(&control),
        "control sessions: {control}"
    );
}

#[test]
fn held_ids_are_read_from_tagged_text_in_a_json_body() {
    let body = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"<system-reminder>\n[lesson:12] [correction] a\n[lesson:7] [convention] b\n[lesson:12] again\n</system-reminder>"}]}]}"#;
    assert_eq!(held_lesson_ids(body), vec![12, 7]);
}

#[test]
fn malformed_tags_are_ignored() {
    assert!(held_lesson_ids(b"[lesson:] [lesson:x] [lesson:5 no close [lesson:").is_empty());
    assert!(held_lesson_ids(b"").is_empty());
}

#[test]
fn held_tags_carry_the_revision_when_the_tag_has_one() {
    let body = b"[lesson:12 rev:3] a [lesson:7] b [lesson:12 rev:3] again [lesson:12 rev:4] c";
    assert_eq!(
        held_lesson_tags(body, &crate::conversation::WHOLE_BODY),
        vec![
            HeldTag {
                id: 12,
                revision: Some(3)
            },
            HeldTag {
                id: 7,
                revision: None
            },
            HeldTag {
                id: 12,
                revision: Some(4)
            },
        ]
    );
    assert_eq!(held_lesson_ids(body), vec![12, 7]);
}

#[test]
fn malformed_revisions_are_ignored() {
    assert!(
        held_lesson_tags(
            b"[lesson:5 rev:] [lesson:5 rev:x] [lesson:5 rev:2 [lesson:5 rev]",
            &crate::conversation::WHOLE_BODY
        )
        .is_empty()
    );
}

#[test]
fn one_held_tag_does_not_make_the_client_a_lesson_holder() {
    let snapshot = LessonSnapshot::default();
    assert!(!client_holds_lessons(true, &snapshot, "s1"));
    snapshot.relevance.note_mod_session("s2");
    assert!(client_holds_lessons(true, &snapshot, "s2"));
    assert!(!client_holds_lessons(false, &snapshot, "s2"));
}

fn routed(route: LessonRoute) -> LessonSnapshot {
    let mut snapshot = snapshot();
    for lesson in snapshot.by_project.values_mut().flatten() {
        lesson.route = route;
    }
    snapshot
}

#[test]
fn lessons_routed_standing_or_covered_are_not_selected() {
    for route in [LessonRoute::Standing, LessonRoute::Covered] {
        let snapshot = routed(route);
        assert!(select_for_prompt(request(&snapshot, &[])).is_empty());
    }
    let snapshot = routed(LessonRoute::Injection);
    assert_eq!(select_for_prompt(request(&snapshot, &[])).len(), 2);
}

#[test]
fn lessons_held_at_their_current_revision_are_not_selected() {
    let snapshot = snapshot();
    let current = [HeldTag {
        id: 1,
        revision: Some(1),
    }];
    let mut held = request(&snapshot, &[]);
    held.held_tags = &current;
    let chosen = select_for_prompt(held);
    assert_eq!(chosen.iter().map(|l| l.id).collect::<Vec<_>>(), vec![2]);
    assert_eq!(chosen[0].rev, 1);

    let snapshot = self::snapshot();
    let stale = [HeldTag {
        id: 1,
        revision: Some(0),
    }];
    let mut refresh = request(&snapshot, &[]);
    refresh.held_tags = &stale;
    assert_eq!(select_for_prompt(refresh).len(), 2);

    let snapshot = self::snapshot();
    let untagged_revision = [HeldTag {
        id: 1,
        revision: None,
    }];
    let mut legacy = request(&snapshot, &[]);
    legacy.held_tags = &untagged_revision;
    assert_eq!(select_for_prompt(legacy).len(), 1);
}

#[test]
fn already_present_names_routed_and_held_lessons() {
    let mut snapshot = snapshot();
    if let Some(lesson) = snapshot
        .by_project
        .get_mut("/repo")
        .and_then(|lessons| lessons.get_mut(1))
    {
        lesson.route = LessonRoute::Covered;
    }
    let body = b"{\"text\":\"[lesson:1 rev:1] a\"}";
    assert_eq!(
        already_present_ids(
            &snapshot,
            "s1",
            "/repo",
            body,
            &crate::conversation::WHOLE_BODY
        ),
        vec![1, 2]
    );
    assert_eq!(
        already_present_ids(
            &snapshot,
            "s1",
            "/repo",
            b"{}",
            &crate::conversation::WHOLE_BODY
        ),
        vec![2]
    );
}

#[test]
fn arm_names_are_the_stored_values() {
    assert_eq!(Arm::Mod.as_str(), "mod");
    assert_eq!(Arm::Control.as_str(), "control");
}

fn snapshot() -> LessonSnapshot {
    LessonSnapshot::from_pairs([(
        "/repo".to_owned(),
        vec![
            (
                "correction".to_owned(),
                "run make fmt before committing".to_owned(),
            ),
            (
                "convention".to_owned(),
                "name the toggle by what it does".to_owned(),
            ),
        ],
    )])
}

fn request<'a>(snapshot: &'a LessonSnapshot, held: &'a [i64]) -> PromptLessons<'a> {
    PromptLessons {
        snapshot,
        session_id: "s1",
        project_path: "/repo",
        prompt: "",
        held,
        held_versions: None,
        held_tags: &[],
        limit: 5,
        content_only: false,
    }
}

#[test]
fn lessons_for_a_prompt_carry_their_ids_and_are_given_once() {
    let snapshot = snapshot();
    let first = select_for_prompt(request(&snapshot, &[]));
    assert_eq!(first.len(), 2);
    let second = select_for_prompt(request(&snapshot, &[]));
    assert!(second.is_empty());
}

#[test]
fn changed_revision_and_compaction_can_refresh_retained_lessons() {
    let snapshot = snapshot();
    let first = select_for_prompt(request(&snapshot, &[]));
    let versions = first.iter().map(|l| (l.id, l.revision.clone())).collect();
    snapshot.relevance.forget_session("s1");
    let mut held = request(&snapshot, &[]);
    held.held_versions = Some(&versions);
    assert!(select_for_prompt(held).is_empty());
    let stale = first.iter().map(|l| (l.id, "obsolete".into())).collect();
    let mut refresh = request(&snapshot, &[]);
    refresh.held_versions = Some(&stale);
    assert_eq!(select_for_prompt(refresh).len(), 2);
    snapshot.relevance.forget_session("s1");
    assert_eq!(select_for_prompt(request(&snapshot, &[])).len(), 2);
}

#[test]
fn relevant_installed_procedure_is_offered_without_claiming_content_delivery() -> anyhow::Result<()>
{
    let snapshot = LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![
            (
                "database migration rollout",
                Some("database-migrations"),
                true,
            ),
            ("CSS animation timing", Some("motion"), true),
        ],
    );
    let topics = catalog_for_prompt(&snapshot, "s1", "/repo", "review database migration");
    assert_eq!(topics, ["database-migrations"]);
    Ok(())
}

#[test]
fn a_prompt_matching_no_topic_names_none() {
    let snapshot = LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![("CSS animation timing", Some("motion"), true)],
    );
    assert!(catalog_for_prompt(&snapshot, "s1", "/repo", "rename the config field").is_empty());
}

#[test]
fn lessons_the_client_holds_are_not_sent_again() {
    let snapshot = snapshot();
    let all = select_for_prompt(request(&snapshot, &[]));
    let fresh = LessonSnapshot::from_pairs([(
        "/repo".to_owned(),
        vec![
            (
                "correction".to_owned(),
                "run make fmt before committing".to_owned(),
            ),
            (
                "convention".to_owned(),
                "name the toggle by what it does".to_owned(),
            ),
        ],
    )]);
    let held = vec![all[0].id];
    let remaining = select_for_prompt(request(&fresh, &held));
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0].id, all[0].id);
}

fn topiced_request(snapshot: &LessonSnapshot) -> PromptLessons<'_> {
    PromptLessons {
        content_only: true,
        ..request(snapshot, &[])
    }
}

fn delivered_texts(snapshot: &LessonSnapshot) -> Vec<String> {
    let mut texts: Vec<String> = select_for_prompt(topiced_request(snapshot))
        .into_iter()
        .map(|lesson| lesson.text)
        .collect();
    texts.sort();
    texts
}

#[test]
fn topiced_lesson_without_an_installed_skill_is_delivered_in_content_only_mode() {
    let snapshot = LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![
            ("untopiced lesson", None, false),
            ("topiced lesson with no skill", Some("deploys"), false),
        ],
    );
    assert_eq!(
        delivered_texts(&snapshot),
        vec!["topiced lesson with no skill", "untopiced lesson"]
    );
}

#[test]
fn topiced_lesson_with_an_installed_skill_is_withheld_in_content_only_mode() {
    let snapshot = LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![
            ("untopiced lesson", None, false),
            ("topiced lesson with a skill", Some("migrations"), true),
            ("topiced lesson with no skill", Some("deploys"), false),
        ],
    );
    assert_eq!(
        delivered_texts(&snapshot),
        vec!["topiced lesson with no skill", "untopiced lesson"]
    );
}

#[test]
fn withheld_topiced_lesson_is_delivered_when_content_only_is_off() {
    let snapshot = LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![("topiced lesson with a skill", Some("migrations"), true)],
    );
    let delivered = select_for_prompt(request(&snapshot, &[]));
    assert_eq!(delivered.len(), 1);
}
