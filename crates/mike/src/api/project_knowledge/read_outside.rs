use axum::response::Response;
use serde_json::{Value, json};

use super::knowledge_error;
use crate::state::AppState;
use crate::store::persistence::project_knowledge::REFERENCE_NOT_IN_PROJECT;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn read(
    state: &AppState,
    handle: &PersistHandle,
    project: Option<&str>,
    reference: &str,
) -> Result<Value, Response> {
    match handle.read_project_knowledge(project, reference).await {
        Ok(read) => Ok(read),
        Err(error) if project.is_some() => read_outside(state, handle, reference, error).await,
        Err(error) => Err(knowledge_error(error)),
    }
}

async fn read_outside(
    state: &AppState,
    handle: &PersistHandle,
    reference: &str,
    error: PersistError,
) -> Result<Value, Response> {
    let missing = matches!(&error, PersistError::DataInvariant(message) if message == REFERENCE_NOT_IN_PROJECT);
    if !missing || !state.rerank_service().current_config().search_no_project {
        return Err(knowledge_error(error));
    }
    let Ok(Some(outside)) = handle.no_project_knowledge_project().await else {
        return Err(knowledge_error(error));
    };
    match handle
        .read_project_knowledge(Some(&outside.project_uuid.to_string()), reference)
        .await
    {
        Ok(mut read) => {
            read["outside_project"] = json!(true);
            Ok(read)
        }
        Err(_) => Err(knowledge_error(error)),
    }
}
