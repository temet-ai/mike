//! Tests for [`super::apply_session_lessons`] and its
//! [`super::content_injected_lesson_ids`] helper: session-start lesson
//! injection, including the OD-2 topic/content-injection split.

use super::session_lessons::*;
use super::test_support::{noop_push_deps, user_turn_body};
use super::*;
use crate::extract::request::TurnOrigin;
use crate::lesson_worker::LessonSnapshot;
use crate::store::models::RequestId;
use serde_json::Value;

#[tokio::test]
async fn apply_session_lessons_no_op_when_inject_lessons_false() -> anyhow::Result<()> {
    let cfg = RemindersConfig {
        inject_lessons: false,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let snap = LessonSnapshot::default();
    let outcome = apply_session_lessons(SessionLessonsRequest {
        body_bytes: &body,
        session_id: "session-lessons-test",
        project_path: "proj-a",
        is_tool_loop: false,
        turn_origin: &TurnOrigin::Primary,
        snapshot: &snap,
        cfg: &cfg,
        recall_injector_enabled: false,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when inject_lessons is false — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_session_lessons_no_op_when_project_path_empty() -> anyhow::Result<()> {
    let cfg = RemindersConfig {
        inject_lessons: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    // Even with a populated snapshot, an empty project_path means
    // there is no key to look lessons up under → Unchanged.
    let snap = LessonSnapshot::from_pairs([(
        "proj-a".to_string(),
        vec![(
            "correction".to_string(),
            "always use make targets".to_string(),
        )],
    )]);
    let outcome = apply_session_lessons(SessionLessonsRequest {
        body_bytes: &body,
        session_id: "session-lessons-test",
        project_path: "",
        is_tool_loop: false,
        turn_origin: &TurnOrigin::Primary,
        snapshot: &snap,
        cfg: &cfg,
        recall_injector_enabled: false,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when project_path is empty — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_session_lessons_no_op_on_tool_loop_continuation() -> anyhow::Result<()> {
    let cfg = RemindersConfig {
        inject_lessons: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    // Snapshot HAS a lesson — proves the tool_loop gate fires before the
    // snapshot read.
    let snap = LessonSnapshot::from_pairs([(
        "proj-a".to_string(),
        vec![(
            "correction".to_string(),
            "always use make targets".to_string(),
        )],
    )]);
    // A tool_loop continuation on a Primary turn must not re-inject lessons.
    let outcome = apply_session_lessons(SessionLessonsRequest {
        body_bytes: &body,
        session_id: "session-lessons-test",
        project_path: "proj-a",
        is_tool_loop: true,
        turn_origin: &TurnOrigin::Primary,
        snapshot: &snap,
        cfg: &cfg,
        recall_injector_enabled: false,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged on a tool_loop continuation — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_session_lessons_no_op_on_non_primary_turn() -> anyhow::Result<()> {
    let cfg = RemindersConfig {
        inject_lessons: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    // Snapshot HAS a lesson — proves the turn_origin gate fires before
    // the snapshot read.
    let snap = LessonSnapshot::from_pairs([(
        "proj-a".to_string(),
        vec![(
            "correction".to_string(),
            "always use make targets".to_string(),
        )],
    )]);
    // Subagent, sidecar, and probe turns must never inject lessons.
    for origin in [
        TurnOrigin::Subagent,
        TurnOrigin::TitleSidecar,
        TurnOrigin::CountTokensSidecar,
        TurnOrigin::QuotaProbe,
    ] {
        let outcome = apply_session_lessons(SessionLessonsRequest {
            body_bytes: &body,
            session_id: "session-lessons-test",
            project_path: "proj-a",
            is_tool_loop: false,
            turn_origin: &origin,
            snapshot: &snap,
            cfg: &cfg,
            recall_injector_enabled: false,
            request_id: RequestId::next(),
            push_deps: &noop_push_deps(),
        })
        .await;
        match outcome {
            RemindersOutcome::Unchanged => {}
            other => {
                return Err(anyhow::anyhow!(
                    "expected Unchanged for non-Primary turn_origin {:?} — got {other:?}",
                    origin
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn apply_session_lessons_injects_from_snapshot_on_first_request() -> anyhow::Result<()> {
    // Pure in-memory proof that the snapshot read path injects — no
    // DB involved. The hot-path read is `snapshot.for_project(..)`.
    let cfg = RemindersConfig {
        inject_lessons: true,
        max_lessons_to_inject: 10,
        ..Default::default()
    };
    let body = serde_json::to_vec(
        &serde_json::json!({"messages":[{"role":"user","content":[{"type":"text","text":"make targets sweep_at_startup"}]}]}),
    )?;
    let snap = LessonSnapshot::from_pairs([(
        "my-project".to_string(),
        vec![
            (
                "correction".to_string(),
                "always use make targets".to_string(),
            ),
            (
                "warning".to_string(),
                "do not touch sweep_at_startup".to_string(),
            ),
        ],
    )]);
    let outcome = apply_session_lessons(SessionLessonsRequest {
        body_bytes: &body,
        session_id: "session-lessons-test",
        project_path: "my-project",
        is_tool_loop: false,
        turn_origin: &TurnOrigin::PrimaryFirst,
        snapshot: &snap,
        cfg: &cfg,
        recall_injector_enabled: false,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Appended {
            body,
            reminder_text,
            source_finding_ids,
            ..
        } => {
            assert!(reminder_text.contains("always use make targets"));
            assert!(reminder_text.contains("do not touch sweep_at_startup"));
            let new_body: Value = serde_json::from_slice(&body)?;
            let last_content = new_body["messages"][0]["content"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
            assert_eq!(last_content.len(), 2);
            assert_eq!(last_content[1]["text"], reminder_text);
            // `from_pairs` assigns fixture ids sequentially (1-based) in
            // the order lessons are listed -- both lessons here have no
            // topic, so both count as content-injected.
            assert_eq!(
                source_finding_ids,
                vec![1i64, 2],
                "source_finding_ids must carry exactly the injected lesson ids"
            );
            Ok(())
        }
        other => Err(anyhow::anyhow!(
            "expected Appended when snapshot has lessons for the project on a fresh session — got {other:?}"
        )),
    }
}

#[test]
fn content_injected_lesson_ids_excludes_withheld_topic_lessons_when_recall_injector_is_on() {
    let snapshot = crate::lesson_worker::LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![
            ("always use make targets", None, false),
            ("prefer set-based SQL", Some("sql-style"), true),
            ("name toggles by what they do", Some("toggle-naming"), false),
        ],
    );
    let lessons = snapshot.for_project("/repo");
    let all_lessons: Vec<_> = lessons.iter().collect();

    assert_eq!(
        content_injected_lesson_ids(&all_lessons, &snapshot, false),
        vec![1i64, 2, 3]
    );
    assert_eq!(
        content_injected_lesson_ids(&all_lessons, &snapshot, true),
        vec![1i64, 3]
    );
}

#[test]
fn reminder_text_catalogs_only_withheld_topics_and_gives_the_rest_in_full() {
    let snapshot = crate::lesson_worker::LessonSnapshot::from_topic_fixtures(
        "/repo",
        vec![
            ("prefer set-based SQL", Some("sql-style"), true),
            ("name toggles by what they do", Some("toggle-naming"), false),
        ],
    );
    let lessons = snapshot.for_project("/repo");
    let all_lessons: Vec<_> = lessons.iter().collect();
    let text = build_session_lesson_reminder_text(
        "catalogs-withheld-topics",
        &all_lessons,
        &snapshot,
        true,
    )
    .unwrap_or_default();
    assert!(text.contains("name toggles by what they do"), "{text}");
    assert!(text.contains("sql-style"), "{text}");
    assert!(!text.contains("prefer set-based SQL"), "{text}");
    assert!(!text.contains("toggle-naming"), "{text}");
}
