use serde::Serialize;
use uuid::Uuid;

use super::ChangesQuery;
use super::resolve::resolve;
use crate::change_record::answers::{self, Caller};
use crate::config::AppState;
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::commit::ownership;
use crate::launch::fleet::seated::Seated;
use crate::store::persistence::PersistHandle;

#[derive(Serialize)]
pub(crate) struct WithTail<T: Serialize> {
    #[serde(flatten)]
    pub(crate) report: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) since_you_last_looked: Option<Vec<String>>,
}

pub(crate) async fn since_you_last_looked(
    state: &AppState,
    definition: &FleetDefinition,
    run: &str,
    session: Uuid,
) -> Option<Vec<String>> {
    if !state.feature_toggle_overlay.load().change_record_tail {
        return None;
    }
    let persist = state.persist()?;
    let seated = Seated::of_run(persist, run, definition).await;
    lines_for(persist, definition, &seated, session).await
}

pub(super) async fn lines_for(
    persist: &PersistHandle,
    definition: &FleetDefinition,
    seated: &Seated,
    session: Uuid,
) -> Option<Vec<String>> {
    let identity = persist.session_identity_by_uuid(session).await.ok()??;
    let owner = ownership(definition, seated, session);
    let caller = Caller {
        session_id: identity.id,
        checkout: definition.repo.clone(),
        in_fleet: true,
        task_files: owner.as_ref().map(|o| o.owned.clone()).unwrap_or_default(),
    };
    let task = owner.map(|o| o.task);
    let mut lines = answers::task_lines(persist, &caller, task.as_deref())
        .await
        .unwrap_or_default();
    lines.extend(read_tail(persist, &caller).await.unwrap_or_default());
    (!lines.is_empty()).then_some(lines)
}

async fn read_tail(persist: &PersistHandle, caller: &Caller) -> Option<Vec<String>> {
    let lines = match answers::tail(persist, caller).await {
        Ok(lines) => lines,
        Err(error) => {
            tracing::warn!(%error, "the lines since a seat last looked could not be read");
            return None;
        }
    };
    (!lines.is_empty()).then_some(lines)
}

pub(crate) async fn note_answer_tail(state: &AppState, session: Uuid) -> Option<Vec<String>> {
    if !state.feature_toggle_overlay.load().change_record_tail {
        return None;
    }
    session_tail_lines(state.persist()?, session).await
}

pub(crate) async fn session_tail_lines(
    persist: &PersistHandle,
    session: Uuid,
) -> Option<Vec<String>> {
    let query = ChangesQuery {
        session: Some(session.to_string()),
        ..ChangesQuery::default()
    };
    let caller = match resolve(persist, &query).await {
        Ok(resolved) => resolved.caller().ok()?,
        Err(error) => {
            tracing::debug!(%error, %session, "no checkout for the lines since a session last looked");
            return None;
        }
    };
    read_tail(persist, &caller).await
}
