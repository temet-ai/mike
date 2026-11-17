use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use super::types::{ConditionError, NewCondition};
use crate::launch::fleet::events::EventKind;

pub const SQL_MAX_CHARS: usize = 4000;
pub const WORDS_MAX_CHARS: usize = 1000;
pub const MIN_CHECK_EVERY_SECS: i32 = 60;
pub const KINDS: [&str; 6] = [
    "time",
    "session_turn_ended",
    "session_ended",
    "fleet_event",
    "sql",
    "words",
];
pub const ACTIONS: [&str; 3] = ["return", "start", "message"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeSpec {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSpec {
    pub session_uuid: Uuid,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetEventSpec {
    pub run_id: String,
    pub kind: String,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub session_uuid: Option<Uuid>,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlExpect {
    #[default]
    Rows,
    NoRows,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlSpec {
    pub query: String,
    #[serde(default)]
    pub expect: SqlExpect,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordsSpec {
    pub text: String,
    pub session_uuid: Uuid,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    Time,
    SessionTurnEnded(SessionSpec),
    SessionEnded(SessionSpec),
    FleetEvent(FleetEventSpec),
    Sql(SqlSpec),
    Words(WordsSpec),
}

fn words(spec: WordsSpec) -> Result<Spec, ConditionError> {
    if spec.text.trim().is_empty() {
        return Err(invalid(
            "words text is empty; say in plain words what must be true".to_owned(),
        ));
    }
    let chars = spec.text.chars().count();
    if chars > WORDS_MAX_CHARS {
        return Err(invalid(format!(
            "words text is {chars} characters; the limit is {WORDS_MAX_CHARS}"
        )));
    }
    Ok(Spec::Words(spec))
}

fn invalid(message: String) -> ConditionError {
    ConditionError::Invalid(message)
}

fn decode<T: DeserializeOwned>(kind: &str, spec: &Value) -> Result<T, ConditionError> {
    serde_json::from_value(spec.clone()).map_err(|e| {
        invalid(format!(
            "spec for kind `{kind}` is invalid: {e}; spec was {spec}"
        ))
    })
}

pub fn validate_sql(query: &str) -> Result<(), ConditionError> {
    if query.trim().is_empty() {
        return Err(invalid("sql query is empty; give one SELECT".to_owned()));
    }
    let chars = query.chars().count();
    if chars > SQL_MAX_CHARS {
        return Err(invalid(format!(
            "sql query is {chars} characters; the limit is {SQL_MAX_CHARS}"
        )));
    }
    if let Some(at) = query.find(';') {
        return Err(invalid(format!(
            "sql query has `;` at byte {at}; give one SELECT with no semicolon, it runs read-only inside EXISTS(...)"
        )));
    }
    Ok(())
}

fn fleet_event(spec: FleetEventSpec) -> Result<Spec, ConditionError> {
    if EventKind::parse(&spec.kind).is_none() {
        let known: Vec<&str> = EventKind::ALL.iter().map(|k| k.as_str()).collect();
        return Err(invalid(format!(
            "fleet event kind `{}` is not one of {}",
            spec.kind,
            known.join(", ")
        )));
    }
    Ok(Spec::FleetEvent(spec))
}

impl Spec {
    pub fn parse(kind: &str, spec: &Value) -> Result<Self, ConditionError> {
        match kind {
            "time" => decode::<TimeSpec>(kind, spec).map(|_| Self::Time),
            "session_turn_ended" => decode(kind, spec).map(Self::SessionTurnEnded),
            "session_ended" => decode(kind, spec).map(Self::SessionEnded),
            "fleet_event" => fleet_event(decode(kind, spec)?),
            "sql" => {
                let parsed: SqlSpec = decode(kind, spec)?;
                validate_sql(&parsed.query)?;
                Ok(Self::Sql(parsed))
            }
            "words" => words(decode(kind, spec)?),
            other => Err(invalid(format!(
                "kind `{other}` is not one of {}",
                KINDS.join(", ")
            ))),
        }
    }

    pub fn since(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::SessionTurnEnded(s) | Self::SessionEnded(s) => s.since,
            Self::FleetEvent(s) => s.since,
            Self::Words(s) => s.since,
            Self::Time | Self::Sql(_) => None,
        }
    }
}

fn validate_action(new: &NewCondition) -> Result<(), ConditionError> {
    if !ACTIONS.contains(&new.action.as_str()) {
        return Err(invalid(format!(
            "action `{}` is not one of {}",
            new.action,
            ACTIONS.join(", ")
        )));
    }
    let has_target = new.target_session.is_some() || new.created_by_session.is_some();
    let has_start_place = has_target || new.folder.as_deref().is_some_and(|f| !f.trim().is_empty());
    let ok = match new.action.as_str() {
        "start" => has_start_place,
        _ => has_target,
    };
    if ok {
        return Ok(());
    }
    Err(invalid(format!(
        "action `{}` needs {}",
        new.action,
        if new.action == "start" {
            "a folder or a target_session"
        } else {
            "a target_session or the registering session_id"
        }
    )))
}

pub fn validate_new(new: &NewCondition) -> Result<Spec, ConditionError> {
    let spec = Spec::parse(&new.kind, &new.spec)?;
    validate_action(new)?;
    if new.brief.trim().is_empty() {
        return Err(invalid("brief must not be blank".to_owned()));
    }
    if new.check_every_secs < MIN_CHECK_EVERY_SECS {
        return Err(invalid(format!(
            "check_every_secs is {}; the minimum is {MIN_CHECK_EVERY_SECS}",
            new.check_every_secs
        )));
    }
    if let Some(deadline) = new.deadline
        && deadline <= new.not_before
    {
        return Err(invalid(format!(
            "deadline {deadline} is not after not_before {}",
            new.not_before
        )));
    }
    Ok(spec)
}
