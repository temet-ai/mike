use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::resolve::session;
use super::{database, mail, problem, text, unavailable};
use crate::launch::fleet::brief_notes_sync::amend_part;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::questions::{
    NoteRef, QuestionRow, SettleKind, SettleOutcome, SettledQuestion,
};

#[derive(Debug, Deserialize)]
pub(crate) struct AnswerBody {
    pub from: String,
    pub answer: String,
    #[serde(default)]
    pub amend: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeclineBody {
    pub from: String,
    #[serde(default)]
    pub reason: Option<String>,
}

async fn known(persist: &PersistHandle, id: Uuid) -> Result<QuestionRow, Box<Response>> {
    match persist.launch().question_by_uuid(id).await {
        Ok(Some(row)) => Ok(row),
        Ok(None) => Err(Box::new(problem(
            StatusCode::NOT_FOUND,
            format!("no question {id}"),
        ))),
        Err(err) => Err(Box::new(database(err))),
    }
}

async fn brief_to_amend(
    persist: &PersistHandle,
    row: &QuestionRow,
) -> Result<NoteRef, Box<Response>> {
    let Some(note_id) = row.about_note else {
        return Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            "amend needs a question that is about a brief note; this one is about none",
        )));
    };
    match persist.launch().note_ref(note_id).await {
        Ok(Some(note))
            if note
                .tags
                .iter()
                .any(|t| t == "part:task" || t == "part:run") =>
        {
            Ok(note)
        }
        Ok(_) => Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            "amend needs the question's note to be a part of a brief (tag part:task or part:run)",
        ))),
        Err(err) => Err(Box::new(database(err))),
    }
}

fn already(row: QuestionRow) -> Response {
    problem(
        StatusCode::CONFLICT,
        format!(
            "question {} is already {}; nothing was changed",
            row.question_uuid, row.state
        ),
    )
}

async fn amended(
    persist: &PersistHandle,
    note: Option<&NoteRef>,
    (row, by): (&QuestionRow, &str),
) -> Result<Option<(Uuid, i64)>, String> {
    let Some(note) = note else {
        return Ok(None);
    };
    let stored = amend_part(persist, note, &text::amendment_entry(row, by)).await?;
    Ok(Some((stored.note, stored.version)))
}

pub(crate) async fn post_question_answer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AnswerBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let by = match session(persist, &body.from, "from").await {
        Ok(by) => by,
        Err(response) => return *response,
    };
    let note = if body.amend {
        let row = match known(persist, id).await {
            Ok(row) => row,
            Err(response) => return *response,
        };
        match brief_to_amend(persist, &row).await {
            Ok(note) => Some(note),
            Err(response) => return *response,
        }
    } else {
        None
    };
    let outcome = persist
        .launch()
        .settle_question(&SettledQuestion {
            question: id,
            by,
            kind: SettleKind::Answered,
            answer: Some(&body.answer),
        })
        .await;
    match outcome {
        Ok(SettleOutcome::Settled(row)) => {
            crate::launch::fleet::questions::record_answered(persist, &row).await;
            settled_answer(persist, *row, by, note).await
        }
        Ok(SettleOutcome::AlreadySettled(row)) => already(*row),
        Ok(SettleOutcome::NotFound) => problem(StatusCode::NOT_FOUND, format!("no question {id}")),
        Err(err) => problem(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn settled_answer(
    persist: &PersistHandle,
    row: QuestionRow,
    by: Uuid,
    note: Option<NoteRef>,
) -> Response {
    let name = text::who(None, by);
    let amendment = amended(persist, note.as_ref(), (&row, &name)).await;
    let (version, amend_error) = match &amendment {
        Ok(version) => (*version, None),
        Err(problem) => (None, Some(problem.clone())),
    };
    let mailed = mail::send(
        persist,
        by,
        row.asker,
        &text::answered_mail(&row, &name, version),
    )
    .await;
    Json(json!({
        "question_id": row.question_uuid,
        "state": row.state,
        "mailed_asker": mailed,
        "amended_note": version.map(|(note, _)| note),
        "amended_version": version.map(|(_, version)| version),
        "amend_error": amend_error,
    }))
    .into_response()
}

pub(crate) async fn post_question_decline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<DeclineBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let by = match session(persist, &body.from, "from").await {
        Ok(by) => by,
        Err(response) => return *response,
    };
    let outcome = persist
        .launch()
        .settle_question(&SettledQuestion {
            question: id,
            by,
            kind: SettleKind::Declined,
            answer: body.reason.as_deref(),
        })
        .await;
    match outcome {
        Ok(SettleOutcome::Settled(row)) => {
            let name = text::who(None, by);
            let mailed =
                mail::send(persist, by, row.asker, &text::declined_mail(&row, &name)).await;
            Json(json!({
                "question_id": row.question_uuid,
                "state": row.state,
                "mailed_asker": mailed,
            }))
            .into_response()
        }
        Ok(SettleOutcome::AlreadySettled(row)) => already(*row),
        Ok(SettleOutcome::NotFound) => problem(StatusCode::NOT_FOUND, format!("no question {id}")),
        Err(err) => problem(StatusCode::BAD_REQUEST, err.to_string()),
    }
}
