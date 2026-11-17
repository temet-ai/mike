use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::knowledge_error;
use crate::semantic::KnowledgeState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::location_move::LocationMove;
use crate::web_base::persist_unavailable_response;

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub machine: Option<Uuid>,
}

fn refusal(status: StatusCode, message: String) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

async fn machine_of(request: &MoveRequest) -> Result<Uuid, Response> {
    if let Some(machine) = request.machine {
        return Ok(machine);
    }
    crate::machine_identity::load_default()
        .await
        .map(|identity| identity.machine_uuid)
        .map_err(|error| {
            refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("this server's machine identity is unavailable ({error}); pass `machine`"),
            )
        })
}

async fn location_named(
    persist: &PersistHandle,
    machine: Uuid,
    from: &str,
) -> Result<Uuid, Response> {
    if let Ok(location) = Uuid::parse_str(from) {
        return Ok(location);
    }
    persist
        .location_at(machine, from)
        .await
        .map_err(knowledge_error)?
        .ok_or_else(|| {
            refusal(
                StatusCode::NOT_FOUND,
                format!(
                    "`from` = `{from}` is not a location on machine {machine}; give its registered root_path or location_uuid (list_project_locations shows both)"
                ),
            )
        })
}

pub async fn move_location(
    State(state): State<KnowledgeState>,
    Json(request): Json<MoveRequest>,
) -> Result<Json<LocationMove>, Response> {
    let persist = state.persist().ok_or_else(persist_unavailable_response)?;
    let machine = machine_of(&request).await?;
    let location = location_named(persist, machine, &request.from).await?;
    persist
        .move_location(location, &request.to)
        .await
        .map(Json)
        .map_err(knowledge_error)
}
