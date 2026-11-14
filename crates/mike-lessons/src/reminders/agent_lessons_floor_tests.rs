use serde_json::json;

use super::RemindersOutcome;
use super::agent_lessons::{AgentLessonsRequest, apply_agent_lessons_with_floor};
use super::supply_floor::rows_tests::{asked_bodies, decider, reply, stub};
use super::test_support::{noop_push_deps, subagent_body_with_system};
use crate::extract::request::TurnOrigin;
use crate::lesson_worker::LessonSnapshot;
use crate::llm::decisions::point::Decider;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

const FIRST: &str = "never approve without running tests";
const SECOND: &str = "prefer small diffs";

fn snapshot() -> LessonSnapshot {
    LessonSnapshot::from_agent_fixtures(
        [("code-reviewer".to_owned(), 7_i64)],
        [(
            7_i64,
            vec![
                ("correction".to_owned(), FIRST.to_owned()),
                ("convention".to_owned(), SECOND.to_owned()),
            ],
        )],
    )
}

async fn run(session: &str, floor: Option<&Decider>) -> anyhow::Result<RemindersOutcome> {
    let body = subagent_body_with_system()?;
    Ok(apply_agent_lessons_with_floor(
        AgentLessonsRequest {
            body_bytes: &body,
            session_id: session,
            subagent_persona: Some("code-reviewer"),
            turn_origin: &TurnOrigin::Subagent,
            snapshot: &snapshot(),
            agent_lessons_enabled: true,
            request_id: RequestId::next(),
            push_deps: &noop_push_deps(),
        },
        floor,
    )
    .await)
}

fn appended(outcome: RemindersOutcome) -> anyhow::Result<(String, Vec<i64>)> {
    match outcome {
        RemindersOutcome::Appended {
            reminder_text,
            source_finding_ids,
            ..
        } => Ok((reminder_text, source_finding_ids)),
        other => Err(anyhow::anyhow!("expected Appended, got {other:?}")),
    }
}

#[tokio::test]
async fn agent_lessons_with_the_point_off_supply_every_lesson() -> anyhow::Result<()> {
    let (text, ids) = appended(run(&uuid::Uuid::new_v4().to_string(), None).await?)?;
    assert_eq!(ids, vec![1, 2]);
    assert!(text.contains(FIRST) && text.contains(SECOND));
    Ok(())
}

#[tokio::test]
async fn agent_lessons_with_the_point_on_supply_only_lessons_at_the_floor() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(&[(1, 0.9), (2, 0.1)])).await;
    let decider = decider(&server, &persist)?;
    let (text, ids) = appended(run(&uuid::Uuid::new_v4().to_string(), Some(&decider)).await?)?;
    assert_eq!(ids, vec![1]);
    assert!(text.contains(FIRST) && !text.contains(SECOND));
    let bodies = asked_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].contains("review this diff"));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn agent_lessons_with_every_lesson_below_the_floor_supply_nothing() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(&[(1, 0.1), (2, 0.2)])).await;
    let decider = decider(&server, &persist)?;
    let outcome = run(&uuid::Uuid::new_v4().to_string(), Some(&decider)).await?;
    assert!(matches!(outcome, RemindersOutcome::Unchanged));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn agent_lessons_with_a_failing_call_supply_every_lesson() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let (text, ids) = appended(run(&uuid::Uuid::new_v4().to_string(), Some(&decider)).await?)?;
    assert_eq!(ids, vec![1, 2]);
    assert!(text.contains(FIRST) && text.contains(SECOND));
    persist.pool().close().await;
    db.cleanup().await
}
