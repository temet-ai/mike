use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use super::definition_view::DefinitionError;
use super::tree::{historical_run, running};
use crate::api::changes::{WithTail, since_you_last_looked};
use crate::launch::fleet::check::{CheckReport, CheckRequest, Runner, check};
use crate::launch::fleet::discovery::{Discovery, DiscoveryArgs, discover};
use crate::launch::fleet::gate::{GateReport, GateRequest, gate};
use crate::launch::fleet::seated::Seated;
use crate::state::AppState;

pub(crate) async fn check_fleet(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(request): Json<CheckRequest>,
) -> Result<Json<WithTail<CheckReport>>, DefinitionError> {
    let (row, def, run) = if request.status {
        historical_run(&state, &key).await?
    } else {
        running(&state, &key).await?
    };
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let runner = Runner {
        persist,
        tracker: &state.tracker,
        run: &run,
        fleet: row.fleet_uuid,
    };
    let seated = Seated::of_run(persist, &run, &def).await;
    let prepared = crate::launch::fleet::ruling::prepare(&runner, &def, &seated, &request)
        .await
        .map_err(DefinitionError::InvalidRequest)?;
    let report = match &prepared {
        Some(prepared) => {
            check(
                Runner {
                    run: &prepared.ledger,
                    ..runner
                },
                &prepared.definition,
                &seated,
                &prepared.request,
            )
            .await
        }
        None => check(runner, &def, &seated, &request).await,
    };
    let since_you_last_looked = if request.status {
        None
    } else {
        since_you_last_looked(&state, &def, &run, request.session).await
    };
    Ok(Json(WithTail {
        report,
        since_you_last_looked,
    }))
}

pub(crate) async fn gate_fleet(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(request): Json<GateRequest>,
) -> Result<Json<GateReport>, DefinitionError> {
    let (row, def, run) = if request.status {
        historical_run(&state, &key).await?
    } else {
        running(&state, &key).await?
    };
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let runner = Runner {
        persist,
        tracker: &state.tracker,
        run: &run,
        fleet: row.fleet_uuid,
    };
    let seated = Seated::of_run(persist, &run, &def).await;
    Ok(Json(gate(runner, &def, &seated, &request).await))
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct WhereQuery {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

pub(crate) async fn where_in_fleet(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<WhereQuery>,
) -> Result<Json<Discovery>, DefinitionError> {
    let (_, def, _) = historical_run(&state, &key).await?;
    let symbol = query
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if symbol.is_none() && path.is_none() {
        return Err(DefinitionError::InvalidRequest(
            "name a symbol, a path, or both".to_string(),
        ));
    }
    let args = DiscoveryArgs {
        repo: def.repo.into(),
        symbol: symbol.map(str::to_string),
        path: path.map(str::to_string),
        context_lines: 8,
    };
    discover(&args)
        .await
        .map(Json)
        .map_err(|error| DefinitionError::InvalidRequest(error.to_string()))
}
