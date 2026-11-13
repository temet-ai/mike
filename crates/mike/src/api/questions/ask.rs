use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::resolve::{Destination, destination, session};
use super::{database, problem, unavailable};
use crate::launch::fleet::questions::ask_and_mail;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::questions::{AskedQuestion, NoteRef};

#[derive(Debug, Deserialize)]
pub(crate) struct AskBody {
    pub from: String,
    pub to: String,
    pub question: String,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub about: Option<String>,
}

async fn about_note(
    persist: &PersistHandle,
    about: Option<&str>,
) -> Result<Option<NoteRef>, Box<Response>> {
    let Some(raw) = about.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok(None);
    };
    let Ok(id) = Uuid::parse_str(raw) else {
        return Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            format!("about `{raw}` is not a note id"),
        )));
    };
    match persist.launch().note_ref(id).await {
        Ok(Some(note)) => Ok(Some(note)),
        Ok(None) => Err(Box::new(problem(
            StatusCode::NOT_FOUND,
            format!("no note {id}"),
        ))),
        Err(err) => Err(Box::new(database(err))),
    }
}

async fn run_of(persist: &PersistHandle, asker: Uuid) -> Option<String> {
    persist
        .fleet()
        .find_fleet_header_for_session(asker)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.run_id)
}

struct Prepared {
    asker: Uuid,
    to: Destination,
    note: Option<NoteRef>,
}

async fn prepare(persist: &PersistHandle, body: &AskBody) -> Result<Prepared, Box<Response>> {
    if body.question.trim().is_empty() {
        return Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            "question must not be empty",
        )));
    }
    let asker = session(persist, &body.from, "from").await?;
    let to = destination(persist, asker, &body.to).await?;
    if let Some(closed) = persist
        .launch()
        .closed_fleet_seat(to.session)
        .await
        .map_err(|err| Box::new(database(err)))?
    {
        return Err(Box::new(crate::api::handlers::mailbox_error(
            StatusCode::CONFLICT,
            "recipient_closed",
            closed.refusal(to.session),
        )));
    }
    let note = about_note(persist, body.about.as_deref()).await?;
    Ok(Prepared { asker, to, note })
}

pub(crate) async fn post_question(
    State(state): State<AppState>,
    Json(body): Json<AskBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let prepared = match prepare(persist, &body).await {
        Ok(prepared) => prepared,
        Err(response) => return *response,
    };
    let run = run_of(persist, prepared.asker).await;
    let new = AskedQuestion {
        asker: prepared.asker,
        to_session: Some(prepared.to.session),
        to_task: prepared.to.task.as_deref(),
        run_id: run.as_deref(),
        about_note: prepared.note.as_ref().map(|n| n.note_uuid),
        question: body.question.trim(),
        blocked: body.blocked,
    };
    let (row, mailed) = match ask_and_mail(persist, &new, prepared.note.as_ref()).await {
        Ok(asked) => asked,
        Err(err) => return problem(StatusCode::BAD_REQUEST, err.to_string()),
    };
    (
        StatusCode::CREATED,
        Json(json!({
            "question_id": row.question_uuid,
            "state": row.state,
            "blocked": row.blocked,
            "to_session": row.to_session,
            "to_task": row.to_task,
            "run_id": row.run_id,
            "about": row.about_note,
            "mailed": mailed,
        })),
    )
        .into_response()
}
