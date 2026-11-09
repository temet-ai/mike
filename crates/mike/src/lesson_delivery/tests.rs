use super::select::PromptLessons;
use super::*;
use crate::lesson_worker::LessonSnapshot;

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
fn proxy_stands_back_only_when_the_toggle_is_on_and_the_client_holds_lessons() {
    let snapshot = LessonSnapshot::default();
    let tagged = br#"{"text":"[lesson:3] a"}"#;
    assert!(client_holds_lessons(true, &snapshot, "s1", tagged));
    assert!(!client_holds_lessons(false, &snapshot, "s1", tagged));
    assert!(!client_holds_lessons(true, &snapshot, "s1", b"{}"));
    snapshot.relevance.note_mod_session("s2");
    assert!(client_holds_lessons(true, &snapshot, "s2", b"{}"));
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
        limit: 5,
        content_only: false,
        budget: None,
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

#[test]
fn a_smaller_budget_delivers_fewer_bytes_than_the_default() {
    let default_snapshot = snapshot();
    let default_bytes: usize = select_for_prompt(request(&default_snapshot, &[]))
        .iter()
        .map(|lesson| lesson.text.len())
        .sum();
    let small_snapshot = snapshot();
    let small = select_for_prompt(PromptLessons {
        budget: Some(35),
        ..request(&small_snapshot, &[])
    });
    let small_bytes: usize = small.iter().map(|lesson| lesson.text.len()).sum();
    assert_eq!(small.len(), 1);
    assert!(small_bytes < default_bytes);
    assert!(small_bytes <= 35);
}

#[test]
fn a_zero_budget_delivers_no_lessons() {
    let snapshot = snapshot();
    let delivered = select_for_prompt(PromptLessons {
        budget: Some(0),
        ..request(&snapshot, &[])
    });
    assert!(delivered.is_empty());
}
