use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

use crate::api::poll_answers::{Loaded, MAILBOX_THREADS, Poll, answer_json, json_bytes};

pub(crate) async fn respond_thread_list(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    filter: crate::store::persistence::mailbox::ThreadSummaryFilter,
) -> Result<Json<ThreadListResponse>, Response> {
    thread_list(handle, state, filter)
        .await
        .map(|(list, _)| Json(list))
}

async fn thread_list(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    filter: crate::store::persistence::mailbox::ThreadSummaryFilter,
) -> Result<(ThreadListResponse, Vec<uuid::Uuid>), Response> {
    match handle.launch().fetch_thread_summaries(&filter).await {
        Ok(rows) => {
            let participants = collect_participant_uuids(&rows);
            let list = project_thread_list(handle, state, rows, &participants).await;
            Ok((list, participants))
        }
        Err(e) => {
            tracing::warn!(error = %e, "fetch_thread_summaries failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch threads".to_string(),
            ))
        }
    }
}

async fn project_thread_list(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    rows: Vec<crate::store::persistence::mailbox::ThreadSummaryRow>,
    participant_uuids: &[uuid::Uuid],
) -> ThreadListResponse {
    let now = chrono::Utc::now();
    let stale_threshold_minutes = state.config().mailbox_stale_minutes;
    let identities = match handle
        .launch()
        .fetch_participant_identities(participant_uuids)
        .await
    {
        Ok(id_rows) => build_identity_index(id_rows),
        Err(e) => {
            tracing::warn!(error = %e, "respond_thread_list: identity fetch failed");
            std::collections::HashMap::new()
        }
    };
    let threads: Vec<ThreadSummaryDto> = rows
        .into_iter()
        .map(|r| thread_summary_dto(r, now, stale_threshold_minutes, &identities))
        .collect();
    ThreadListResponse {
        total_count: threads.len(),
        threads,
    }
}

fn collect_participant_uuids(
    rows: &[crate::store::persistence::mailbox::ThreadSummaryRow],
) -> Vec<uuid::Uuid> {
    let set: std::collections::HashSet<uuid::Uuid> = rows
        .iter()
        .flat_map(|r| r.participant_uuids.iter().copied())
        .collect();
    set.into_iter().collect()
}

pub(crate) async fn get_session_threads(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<ThreadListResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        ));
    };
    let filter = crate::store::persistence::mailbox::ThreadSummaryFilter {
        participant_session_uuid: Some(session_uuid),
        ..Default::default()
    };
    respond_thread_list(handle, &state, filter).await
}

fn parse_participant(raw: Option<&str>) -> Result<Option<uuid::Uuid>, Box<Response>> {
    raw.map(|raw| {
        uuid::Uuid::parse_str(raw).map_err(|_| {
            Box::new(mailbox_error(
                StatusCode::BAD_REQUEST,
                "invalid_participant",
                format!("participant is not a valid UUID: {raw}"),
            ))
        })
    })
    .transpose()
}

pub(crate) async fn list_threads(
    State(state): State<AppState>,
    Query(params): Query<ListThreadsParams>,
    headers: HeaderMap,
) -> Response {
    let Some(handle) = state.persist() else {
        return mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        );
    };
    let participant_session_uuid = match parse_participant(params.participant.as_deref()) {
        Ok(participant) => participant,
        Err(response) => return *response,
    };
    let poll = Poll {
        key: format!(
            "threads:{participant_session_uuid:?}:{:?}:{:?}:{:?}",
            params.project, params.resolved, params.tag
        ),
        freshness: MAILBOX_THREADS,
        scope: participant_session_uuid
            .iter()
            .map(ToString::to_string)
            .collect(),
        headers: &headers,
    };
    answer_json(&state, poll, || {
        load_thread_list(handle, &state, participant_session_uuid, params)
    })
    .await
}

async fn load_thread_list(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    participant_session_uuid: Option<uuid::Uuid>,
    params: ListThreadsParams,
) -> Result<Loaded<Bytes>, Response> {
    let project_ids = match params.project.as_deref() {
        Some(path) => match handle.lessons().get_project_ids_for_path(path).await {
            Ok(ids) => Some(ids),
            Err(e) => {
                tracing::warn!(error = %e, project = path, "list_threads: project_path resolution failed");
                return Err(mailbox_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "failed to resolve project".to_string(),
                ));
            }
        },
        None => None,
    };
    let filter = crate::store::persistence::mailbox::ThreadSummaryFilter {
        participant_session_uuid,
        project_ids,
        resolved: params.resolved,
        tag: params.tag,
        ..Default::default()
    };
    let (list, participants) = thread_list(handle, state, filter).await?;
    Ok(Loaded {
        value: json_bytes(&list)?,
        sessions: participants.iter().map(ToString::to_string).collect(),
    })
}
