use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::launch::fleet::decisions::test_support::{
    DecisionsFixture, jev_decider, noul_body, stub_jev,
};
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::session_notes::NewSessionNote;

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        NEEDS.to_owned(),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

#[test]
fn compose_an_answered_noul_gives_the_probability() {
    assert_eq!(compose(&answered(0.2)), Some(0.2));
}

#[test]
fn compose_a_failed_call_gives_nothing() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn routes_to_coordinator_only_below_the_threshold() {
    assert!(routes_to_coordinator(Some(0.1)));
    assert!(!routes_to_coordinator(Some(0.3)));
    assert!(!routes_to_coordinator(Some(0.9)));
    assert!(!routes_to_coordinator(None));
}

#[test]
fn state_keeps_the_end_of_a_long_body() {
    let body = format!("{}END", "x".repeat(4000));
    let asking = Asking {
        source: "operator note",
        seat: "worker-T1",
        title: "which file",
        body: &body,
    };
    let state = state(&asking);
    let tail = state["body_tail"].as_str().unwrap_or_default();
    assert_eq!(tail.chars().count(), BODY_CHARS);
    assert!(tail.ends_with("END"));
    assert_eq!(state["seat"], "worker-T1");
}

async fn raised(fx: &DecisionsFixture) -> anyhow::Result<SessionNoteRow> {
    Ok(fx
        .handle
        .fleet()
        .create_session_note(
            fx.worker,
            NewSessionNote {
                kind: Some("task"),
                title: "OPERATOR: which file owns the parser",
                body: Some("T1 and T2 both list src/parser/"),
                status: Some("operator"),
                tags: None,
                data: None,
            },
        )
        .await?)
}

async fn coordinator_mail(fx: &DecisionsFixture) -> anyhow::Result<usize> {
    let rows = fx
        .handle
        .launch()
        .fetch_for_session_with_activity(fx.coordinator)
        .await?;
    Ok(rows.inbox.len())
}

#[tokio::test]
async fn route_note_a_question_the_coordinator_can_answer_is_tagged_and_mailed()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, noul_body(&[(NEEDS, 0.1)])).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let note = raised(&fx).await?;
    route_note(&decider, &fx.handle, fx.worker, &note).await;
    let after = fx
        .handle
        .launch()
        .fetch_session_notes(
            fx.worker,
            crate::store::persistence::session_notes::SessionNoteFilter {
                kind: None,
                status: None,
                tag: Some(TAG),
                q: None,
                order: crate::store::persistence::session_notes::NoteOrder::NewestFirst,
                limit: 10,
                offset: 0,
            },
        )
        .await?;
    assert_eq!(after.len(), 1);
    assert_eq!(coordinator_mail(&fx).await?, 1);
    fx.finish().await
}

#[tokio::test]
async fn route_note_a_question_for_the_operator_is_left_alone() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, noul_body(&[(NEEDS, 0.9)])).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let note = raised(&fx).await?;
    route_note(&decider, &fx.handle, fx.worker, &note).await;
    assert_eq!(coordinator_mail(&fx).await?, 0);
    fx.finish().await
}

#[tokio::test]
async fn route_note_with_a_failing_client_leaves_the_question_with_the_operator()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(500, json!({})).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let note = raised(&fx).await?;
    route_note(&decider, &fx.handle, fx.worker, &note).await;
    assert_eq!(coordinator_mail(&fx).await?, 0);
    fx.finish().await
}
