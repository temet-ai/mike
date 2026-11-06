use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::definition_view::DefinitionError;
use crate::config::AppState;
use crate::launch::fleet::run_record;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RecordQuery {
    #[serde(default)]
    recompute: bool,
    against: Option<String>,
}

async fn find_row(persist: &PersistHandle, key: &str) -> Result<FleetRow, DefinitionError> {
    persist
        .find_fleet(key)
        .await?
        .ok_or_else(|| DefinitionError::NotFound(key.to_string()))
}

async fn record_of(
    persist: &PersistHandle,
    row: &FleetRow,
    recompute: bool,
) -> Result<Value, DefinitionError> {
    if let (true, Some(run_id)) = (row.state == "done", row.run_id.as_deref()) {
        if recompute {
            run_record::recompute(persist, row, run_id).await?;
        } else if persist.find_run_record(row.fleet_uuid).await?.is_none() {
            run_record::write(persist, row, run_id).await?;
        }
    }
    persist
        .find_run_record(row.fleet_uuid)
        .await?
        .ok_or(DefinitionError::NotFound(format!(
            "{} has no run record: it is written when the run closes",
            row.slug
        )))
}

pub(crate) async fn get_fleet_record(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<RecordQuery>,
) -> Result<Json<Value>, DefinitionError> {
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let row = find_row(persist, &key).await?;
    let record = record_of(persist, &row, query.recompute).await?;
    let Some(other_key) = query.against else {
        return Ok(Json(record));
    };
    let other_row = find_row(persist, &other_key).await?;
    let other = record_of(persist, &other_row, false).await?;
    let difference = run_record::difference(&record, &other);
    Ok(Json(json!({
        "record": record,
        "against": {
            "fleet": other_row.fleet_uuid,
            "record": other,
            "difference": difference,
        },
    })))
}
