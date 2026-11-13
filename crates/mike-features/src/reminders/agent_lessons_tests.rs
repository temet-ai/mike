use super::agent_lessons::*;
use super::test_support::{code_reviewer_snapshot, noop_push_deps, subagent_body_with_system};
use super::*;
use crate::extract::request::TurnOrigin;
use crate::store::models::RequestId;
use serde_json::Value;

#[tokio::test]
async fn apply_agent_lessons_stands_aside_for_persona_served_by_client() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    assert!(snap.mark_agent_lesson_served_by_client("session-served-1", "code-reviewer"));
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-served-1",
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Suppressed {
            reason: "served_by_client",
        } => Ok(()),
        other => Err(anyhow::anyhow!("expected served_by_client, got {other:?}")),
    }
}

#[tokio::test]
async fn apply_agent_lessons_still_injects_for_session_never_served_by_client() -> anyhow::Result<()>
{
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    assert!(snap.mark_agent_lesson_served_by_client("session-served-2", "other-persona"));
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-never-served",
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    assert!(!matches!(outcome, RemindersOutcome::Suppressed { .. }));
    Ok(())
}

async fn outcome_for(
    snap: &crate::lesson_worker::LessonSnapshot,
    session_id: &str,
    persona: Option<&str>,
) -> anyhow::Result<RemindersOutcome> {
    let body = subagent_body_with_system()?;
    Ok(apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id,
        subagent_persona: persona,
        turn_origin: &TurnOrigin::Subagent,
        snapshot: snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await)
}

#[tokio::test]
async fn apply_agent_lessons_served_under_agent_name_matches_persona_of_its_definition()
-> anyhow::Result<()> {
    let snap = crate::lesson_worker::LessonSnapshot::from_agent_fixtures(
        [
            ("db-dev".to_string(), 9_i64),
            (
                "database specialist for mike's capture database".to_string(),
                9_i64,
            ),
        ],
        [(
            9_i64,
            vec![("correction".to_string(), "set-based SQL".to_string())],
        )],
    );
    assert!(snap.mark_agent_lesson_served_by_client("session-served-db-dev", "db-dev"));
    let outcome = outcome_for(
        &snap,
        "session-served-db-dev",
        Some("database specialist for mike's capture database"),
    )
    .await?;
    assert!(
        matches!(
            outcome,
            RemindersOutcome::Suppressed {
                reason: "served_by_client"
            }
        ),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn apply_agent_lessons_unresolved_persona_in_client_served_session_is_served_by_client()
-> anyhow::Result<()> {
    let snap = code_reviewer_snapshot();
    assert!(snap.mark_agent_lesson_served_by_client("session-served-unresolved", "code-reviewer"));
    for persona in [None, Some("Claude agent")] {
        let outcome = outcome_for(&snap, "session-served-unresolved", persona).await?;
        assert!(
            matches!(
                outcome,
                RemindersOutcome::Suppressed {
                    reason: "served_by_client"
                }
            ),
            "{persona:?}: {outcome:?}"
        );
    }
    let elsewhere = outcome_for(&snap, "session-unserved-unresolved", None).await?;
    assert!(
        matches!(elsewhere, RemindersOutcome::Unchanged),
        "{elsewhere:?}"
    );
    Ok(())
}

#[tokio::test]
async fn apply_agent_lessons_no_op_when_toggle_off() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-agent-1",
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: false,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when agent_lessons toggle is off — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_agent_lessons_no_op_on_non_subagent_turn() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    for origin in [
        TurnOrigin::Primary,
        TurnOrigin::PrimaryFirst,
        TurnOrigin::TitleSidecar,
    ] {
        let outcome = apply_agent_lessons(AgentLessonsRequest {
            body_bytes: &body,
            session_id: "session-agent-2",
            subagent_persona: Some("code-reviewer"),
            turn_origin: &origin,
            snapshot: &snap,
            agent_lessons_enabled: true,
            request_id: RequestId::next(),
            push_deps: &noop_push_deps(),
        })
        .await;
        match outcome {
            RemindersOutcome::Unchanged => {}
            other => {
                return Err(anyhow::anyhow!(
                    "expected Unchanged for non-Subagent turn_origin {:?} — got {other:?}",
                    origin
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn apply_agent_lessons_no_op_when_persona_unaliased() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-agent-3",
        subagent_persona: Some("some free-text persona sentence"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when persona has no seeded alias — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_agent_lessons_no_op_when_persona_is_none() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-agent-4",
        subagent_persona: None,
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when subagent_persona is None — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn apply_agent_lessons_injects_once_then_skips_second_call() -> anyhow::Result<()> {
    let body = subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();

    let first = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-agent-5",
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    let (new_body, reminder_text) = match first {
        RemindersOutcome::Appended {
            body,
            reminder_text,
            source_finding_ids,
            ..
        } => {
            assert_eq!(
                source_finding_ids,
                vec![1i64],
                "source_finding_ids must carry exactly the injected lesson id"
            );
            (body, reminder_text)
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Appended on first call for a resolvable persona with lessons — got {other:?}"
            ));
        }
    };
    assert!(reminder_text.contains("never approve without running tests"));
    assert!(reminder_text.contains("code-reviewer"));

    let original: Value = serde_json::from_slice(&body)?;
    let rewritten: Value = serde_json::from_slice(&new_body)?;
    assert_eq!(original["system"], rewritten["system"]);
    let last_content = rewritten["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(last_content.len(), 2);
    assert_eq!(last_content[1]["type"], "text");
    assert_eq!(last_content[1]["text"], reminder_text);

    let second = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: "session-agent-5",
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    match second {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged on the second call for the same (session, persona) — got {other:?}"
        )),
    }
}
