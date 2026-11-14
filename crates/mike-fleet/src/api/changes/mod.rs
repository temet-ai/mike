pub mod code;
pub mod diff;
pub mod mine;
pub mod resolve;
pub mod standing_view;
pub mod tail;

#[cfg(test)]
pub mod seat_tests;
#[cfg(test)]
mod tests;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::base_state::BaseState;
use crate::change_record::answers::{self, AnswerError, Scope, Since};
use crate::store::persistence::PersistError;
use resolve::{Resolved, resolve};

pub use tail::{WithTail, note_answer_tail, session_tail_lines, since_you_last_looked};

pub const CHANGE_RECORD_OFF: &str = "the change record is off, so nothing has been recorded to answer from; switch on `change_record` in /settings";

#[derive(Debug, thiserror::Error)]
pub enum ChangesError {
    #[error("{0}")]
    Invalid(String),
    #[error("no session is recorded as {0}")]
    UnknownSession(String),
    #[error("the database is not reachable, so the change record cannot be read")]
    Unavailable,
    #[error(transparent)]
    Answer(#[from] AnswerError),
    #[error(transparent)]
    Persist(#[from] PersistError),
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for ChangesError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::UnknownSession(_) => (StatusCode::NOT_FOUND, "unknown_session"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "persistence_unavailable"),
            Self::Answer(AnswerError::UnknownCommit(_)) => {
                (StatusCode::BAD_REQUEST, "unknown_commit")
            }
            Self::Answer(_) | Self::Persist(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "change_record_failed")
            }
        };
        let body = ErrorBody {
            error: code,
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ChangesQuery {
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    checkout: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    paths: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    line_limit: Option<usize>,
}

enum View {
    Diff,
    Changes,
    Standing,
    Whose,
    Mine,
    Tail,
    Feed,
}

fn parse_view(text: Option<&str>) -> Result<View, ChangesError> {
    match text.map(str::trim) {
        Some("diff") => Ok(View::Diff),
        Some("changes") => Ok(View::Changes),
        Some("standing") => Ok(View::Standing),
        Some("whose") => Ok(View::Whose),
        Some("mine") => Ok(View::Mine),
        Some("tail") => Ok(View::Tail),
        Some("feed") => Ok(View::Feed),
        other => Err(ChangesError::Invalid(format!(
            "`view` must be changes, standing, whose, mine, diff, tail or feed; got {:?}",
            other.unwrap_or("")
        ))),
    }
}

fn parse_since(text: Option<&str>) -> Result<Since, ChangesError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(Since::Place);
    };
    if text == "place" {
        return Ok(Since::Place);
    }
    if let Ok(at) = text.parse::<DateTime<Utc>>() {
        return Ok(Since::Time(at));
    }
    let hex = text.len() >= 4 && text.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return Ok(Since::Commit(text.to_ascii_lowercase()));
    }
    Err(ChangesError::Invalid(format!(
        "`since` must be `place`, an RFC 3339 time or a commit id of at least four hex digits; got {text:?}"
    )))
}

fn parse_scope(scope: Option<&str>, paths: Option<&str>) -> Result<Scope, ChangesError> {
    let listed: Vec<String> = paths
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    match scope.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("mine") if listed.is_empty() => Ok(Scope::Mine),
        None | Some("paths") if !listed.is_empty() => Ok(Scope::Paths(listed)),
        Some("all") => Ok(Scope::All),
        other => Err(ChangesError::Invalid(format!(
            "`scope` must be mine, all or paths (with `paths` listing the files); got {:?} with {} path(s)",
            other.unwrap_or(""),
            listed.len()
        ))),
    }
}

pub async fn get_changes(
    State(state): State<BaseState>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<Value>, ChangesError> {
    if !state.feature_toggle_overlay.load().change_record {
        return Ok(Json(
            json!({"recording": false, "message": CHANGE_RECORD_OFF}),
        ));
    }
    let view = parse_view(query.view.as_deref())?;
    let persist = state.persist().ok_or(ChangesError::Unavailable)?;
    let resolved = resolve(persist, &query).await?;
    let value = match view {
        View::Diff => diff::diff(persist, &resolved, &query).await?,
        View::Changes => {
            let since = parse_since(query.since.as_deref())?;
            let scope = parse_scope(query.scope.as_deref(), query.paths.as_deref())?;
            let caller = resolved.caller()?;
            let answer = answers::changes(persist, &caller, &since, &scope).await?;
            serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))?
        }
        View::Standing => standing_view::standing(persist, &resolved).await?,
        View::Whose => whose(persist, &resolved, query.path.as_deref()).await?,
        View::Mine => mine::mine(persist, &resolved).await?,
        View::Feed => {
            let answer = answers::feed(persist, &resolved.checkout, answers::FEED_LINES).await?;
            serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))?
        }
        View::Tail if !state.feature_toggle_overlay.load().change_record_tail => {
            json!({ "lines": [], "tail": false })
        }
        View::Tail => {
            let lines = answers::tail(persist, &resolved.caller()?).await?;
            json!({ "lines": lines, "tail": true })
        }
    };
    Ok(Json(value))
}

async fn whose(
    persist: &crate::store::persistence::PersistHandle,
    resolved: &Resolved,
    path: Option<&str>,
) -> Result<Value, ChangesError> {
    let path = path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| ChangesError::Invalid("`view=whose` needs `path`".to_owned()))?;
    let relative = resolved.relative(path);
    let answer = answers::whose(persist, &resolved.checkout, &relative).await?;
    let mut value =
        serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))?;
    if let Some((task, seat)) = resolved.owner_of(&relative) {
        value["task"] = json!(task);
        value["seat"] = json!(seat);
    }
    Ok(value)
}
