use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{new_condition, time_condition};
use crate::store::persistence::conditions::spec::{Spec, SqlExpect, validate_new, validate_sql};
use crate::store::persistence::conditions::{ConditionError, NewCondition};

fn message_of(result: Result<Spec, ConditionError>) -> anyhow::Result<String> {
    match result {
        Err(ConditionError::Invalid(message)) => Ok(message),
        other => anyhow::bail!("expected an Invalid error, got {other:?}"),
    }
}

#[test]
fn spec_parse_session_kind_without_session_uuid_names_the_field() -> anyhow::Result<()> {
    let message = message_of(Spec::parse("session_turn_ended", &json!({})))?;
    assert!(message.contains("session_uuid"), "{message}");
    assert!(message.contains("session_turn_ended"), "{message}");
    Ok(())
}

#[test]
fn spec_parse_fleet_event_without_run_id_is_refused() -> anyhow::Result<()> {
    let message = message_of(Spec::parse(
        "fleet_event",
        &json!({"kind": "check_finished"}),
    ))?;
    assert!(message.contains("run_id"), "{message}");
    Ok(())
}

#[test]
fn spec_parse_fleet_event_with_unknown_kind_lists_the_known_ones() -> anyhow::Result<()> {
    let message = message_of(Spec::parse(
        "fleet_event",
        &json!({"run_id": "r1", "kind": "nonsense"}),
    ))?;
    assert!(message.contains("nonsense"), "{message}");
    assert!(message.contains("check_finished"), "{message}");
    Ok(())
}

#[test]
fn spec_parse_unknown_kind_lists_the_kinds() -> anyhow::Result<()> {
    let message = message_of(Spec::parse("whenever", &json!({})))?;
    assert!(message.contains("whenever"), "{message}");
    assert!(message.contains("session_ended"), "{message}");
    Ok(())
}

#[test]
fn spec_parse_sql_defaults_to_expecting_rows() -> anyhow::Result<()> {
    let spec = Spec::parse("sql", &json!({"query": "SELECT 1"}))?;
    match spec {
        Spec::Sql(sql) => assert_eq!(sql.expect, SqlExpect::Rows),
        other => anyhow::bail!("expected a sql spec, got {other:?}"),
    }
    Ok(())
}

#[test]
fn spec_parse_sql_no_rows_is_read() -> anyhow::Result<()> {
    let spec = Spec::parse("sql", &json!({"query": "SELECT 1", "expect": "no_rows"}))?;
    match spec {
        Spec::Sql(sql) => assert_eq!(sql.expect, SqlExpect::NoRows),
        other => anyhow::bail!("expected a sql spec, got {other:?}"),
    }
    Ok(())
}

#[test]
fn validate_sql_rejects_empty_semicolon_and_long_queries() -> anyhow::Result<()> {
    assert!(validate_sql("   ").is_err());
    let semicolon = validate_sql("SELECT 1; DROP TABLE x");
    let Err(ConditionError::Invalid(message)) = semicolon else {
        anyhow::bail!("a semicolon must be refused");
    };
    assert!(message.contains("byte 8"), "{message}");
    assert!(validate_sql(&"x".repeat(4001)).is_err());
    assert!(validate_sql(&"x".repeat(4000)).is_ok());
    Ok(())
}

fn refused(new: &NewCondition) -> anyhow::Result<String> {
    message_of(validate_new(new))
}

#[test]
fn validate_new_return_action_without_any_target_is_refused() -> anyhow::Result<()> {
    let mut new = time_condition();
    new.action = "return".to_owned();
    new.target_session = None;
    new.created_by_session = None;
    assert!(refused(&new)?.contains("target_session"));
    Ok(())
}

#[test]
fn validate_new_start_action_needs_a_folder_or_a_session() -> anyhow::Result<()> {
    let mut new = time_condition();
    new.action = "start".to_owned();
    new.target_session = None;
    new.created_by_session = None;
    assert!(refused(&new)?.contains("folder"));
    new.folder = Some("/tmp/repo".to_owned());
    assert!(validate_new(&new).is_ok());
    Ok(())
}

#[test]
fn validate_new_refuses_deadline_not_after_not_before() -> anyhow::Result<()> {
    let mut new = time_condition();
    new.deadline = Some(new.not_before - Duration::minutes(1));
    assert!(refused(&new)?.contains("deadline"));
    new.deadline = Some(Utc::now() + Duration::hours(2));
    assert!(validate_new(&new).is_ok());
    Ok(())
}

#[test]
fn validate_new_refuses_short_check_interval_and_blank_brief() -> anyhow::Result<()> {
    let mut new = time_condition();
    new.check_every_secs = 59;
    assert!(refused(&new)?.contains("minimum is 60"));
    let mut blank = new_condition("time", json!({}));
    blank.brief = "  ".to_owned();
    assert!(refused(&blank)?.contains("brief"));
    Ok(())
}

#[test]
fn validate_new_accepts_a_session_condition_with_since() -> anyhow::Result<()> {
    let since = Utc::now() - Duration::hours(3);
    let new = new_condition(
        "session_ended",
        json!({"session_uuid": Uuid::new_v4(), "since": since}),
    );
    let spec = validate_new(&new)?;
    assert_eq!(spec.since().map(|s| s.timestamp()), Some(since.timestamp()));
    Ok(())
}
