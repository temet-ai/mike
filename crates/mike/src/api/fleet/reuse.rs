use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use super::definition_view::DefinitionError;
use super::tree::historical_run;
use crate::launch::fleet::reuse::{ReuseAnswer, ask};
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ReuseQuery {
    #[serde(default)]
    command: Option<String>,
}

pub(crate) async fn get_fleet_reuse(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<ReuseQuery>,
) -> Result<Json<ReuseAnswer>, DefinitionError> {
    let command = query
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            DefinitionError::InvalidRequest("name the command to look up".to_string())
        })?;
    let (_, def, run) = historical_run(&state, &key).await?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    ask(persist, &def, &run, command)
        .await
        .map(Json)
        .map_err(DefinitionError::Status)
}
