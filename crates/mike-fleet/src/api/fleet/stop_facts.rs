use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::tree::running;
use crate::base_state::BaseState;
use crate::launch::fleet::decisions::stop::{StopAnswer, StopRequest, answer};
use crate::launch::fleet::decisions::{FleetDecisions, Mode, Point, stop_facts};
use crate::launch::fleet::seated::Seated;

pub async fn post_stop_facts(
    State(state): State<BaseState>,
    Path((key, session)): Path<(String, String)>,
    body: Option<Json<StopRequest>>,
) -> Result<Json<StopAnswer>, DefinitionError> {
    let session = Uuid::parse_str(session.trim()).map_err(|error| {
        DefinitionError::InvalidRequest(format!("`{session}` is not a session id: {error}"))
    })?;
    let request = body.map(|Json(request)| request).unwrap_or_default();
    let (row, def, run) = running(&state, &key).await?;
    let Some(persist) = state.persist() else {
        return Ok(Json(StopAnswer::off("no database")));
    };
    let seated = Seated::of_run(persist, &run, &def).await;
    let mode = def.session_decision_mode(Point::StopCheck, &seated, session);
    let decisions = FleetDecisions::from_state(&state);
    let layer_on = decisions.as_ref().is_some_and(FleetDecisions::layer_on);
    if mode == Mode::Off || !layer_on {
        return Ok(Json(StopAnswer::off("the stop check is off")));
    }
    if request.probe {
        let task_seat = seated.task_of(session).is_some();
        return Ok(Json(if mode == Mode::On && task_seat {
            StopAnswer::active()
        } else {
            StopAnswer::off("the stop check is not deciding for this seat")
        }));
    }
    let Some(facts) = stop_facts::gather(persist, (&def, &seated), &run, session).await else {
        return Ok(Json(StopAnswer::off(
            "the session holds no task in this fleet",
        )));
    };
    Ok(Json(
        answer(
            decisions.as_ref(),
            mode,
            (facts, request),
            (session, row.id),
        )
        .await,
    ))
}
