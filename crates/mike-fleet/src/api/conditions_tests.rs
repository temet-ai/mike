use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{BuildContext, CreateConditionRequest, build_new};
use crate::store::persistence::conditions::ConditionError;

fn request(body: Value) -> anyhow::Result<CreateConditionRequest> {
    Ok(serde_json::from_value(body)?)
}

fn context(project_id: Option<i64>) -> BuildContext {
    BuildContext {
        machine_uuid: Uuid::new_v4(),
        project_id,
        now: Utc::now(),
    }
}

fn refusal(result: Result<impl std::fmt::Debug, ConditionError>) -> anyhow::Result<String> {
    match result {
        Err(ConditionError::Invalid(message)) => Ok(message),
        other => anyhow::bail!("expected an Invalid error, got {other:?}"),
    }
}

#[test]
fn build_new_defaults_the_action_to_return_and_the_session_into_the_spec() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let req = request(json!({
        "session_id": session,
        "kind": "session_turn_ended",
        "brief": "pick this up",
    }))?;
    let new = build_new(req, &context(None))?;
    assert_eq!(new.action, "return");
    assert_eq!(new.created_by_session, Some(session));
    assert_eq!(new.spec["session_uuid"], json!(session));
    assert_eq!(new.check_every_secs, 300);
    Ok(())
}

#[test]
fn build_new_keeps_a_session_uuid_the_caller_named() -> anyhow::Result<()> {
    let other = Uuid::new_v4();
    let req = request(json!({
        "session_id": Uuid::new_v4(),
        "kind": "session_ended",
        "spec": {"session_uuid": other},
        "brief": "b",
    }))?;
    let new = build_new(req, &context(None))?;
    assert_eq!(new.spec["session_uuid"], json!(other));
    Ok(())
}

#[test]
fn build_new_time_condition_needs_a_start() -> anyhow::Result<()> {
    let req = request(json!({"kind": "time", "brief": "b", "target_session": Uuid::new_v4()}))?;
    let message = refusal(build_new(req, &context(None)))?;
    assert!(message.contains("not_before or after_secs"), "{message}");
    Ok(())
}

#[test]
fn build_new_after_secs_and_deadline_secs_count_from_now() -> anyhow::Result<()> {
    let ctx = context(None);
    let req = request(json!({
        "kind": "time",
        "after_secs": 86400,
        "deadline_secs": 90000,
        "action": "start",
        "folder": "/work/repo",
        "brief": "start the follow-up",
    }))?;
    let new = build_new(req, &ctx)?;
    assert_eq!(new.not_before, ctx.now + Duration::seconds(86400));
    assert_eq!(new.deadline, Some(ctx.now + Duration::seconds(90000)));
    assert_eq!(new.folder.as_deref(), Some("/work/repo"));
    Ok(())
}

#[test]
fn build_new_refuses_negative_offsets_bad_uuids_and_non_object_specs() -> anyhow::Result<()> {
    let negative = request(json!({"kind": "time", "after_secs": -5, "brief": "b"}))?;
    assert!(refusal(build_new(negative, &context(None)))?.contains("after_secs is -5"));

    let bad_uuid =
        request(json!({"kind": "time", "after_secs": 1, "brief": "b", "session_id": "nope"}))?;
    let message = refusal(build_new(bad_uuid, &context(None)))?;
    assert!(message.contains("session_id `nope`"), "{message}");

    let scalar = request(json!({"kind": "sql", "spec": "SELECT 1", "brief": "b"}))?;
    assert!(refusal(build_new(scalar, &context(None)))?.contains("JSON object"));
    Ok(())
}

#[test]
fn build_new_thread_needs_a_resolved_project() -> anyhow::Result<()> {
    let body = json!({
        "kind": "time",
        "after_secs": 60,
        "thread": "side-calls",
        "target_session": Uuid::new_v4(),
        "brief": "b",
    });
    let message = refusal(build_new(request(body.clone())?, &context(None)))?;
    assert!(message.contains("thread needs a project"), "{message}");
    let new = build_new(request(body)?, &context(Some(9)))?;
    assert_eq!(new.thread_slug.as_deref(), Some("side-calls"));
    assert_eq!(new.project_id, Some(9));
    Ok(())
}
