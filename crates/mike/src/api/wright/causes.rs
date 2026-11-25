use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use super::{bad_request, refuse};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{
    CausePatch, GroupWrite, MergeOutcome, PatchOutcome, SeparateOutcome,
};
use crate::web_base::{persist_error_response, persist_unavailable_response};

const PRIORITY_LIMIT: f64 = 1e12;
const GROUP_NAME_MAX_CHARS: usize = 80;
const MERGE_MAX_CAUSES: usize = 200;

fn present<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PatchCauseBody {
    pinned: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    operator_priority: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    group_id: Option<Option<i64>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MergeCausesBody {
    cause_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GroupBody {
    name: String,
}

fn conflict(reason: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": "conflict", "reason": reason})),
    )
        .into_response()
}

fn cause_patch(body: PatchCauseBody) -> Result<CausePatch, String> {
    if let Some(Some(value)) = body.operator_priority
        && !(value.is_finite() && value.abs() <= PRIORITY_LIMIT)
    {
        return Err(format!(
            "operator_priority is {value}; it must be a number between -{PRIORITY_LIMIT:e} and {PRIORITY_LIMIT:e}"
        ));
    }
    let patch = CausePatch {
        pinned: body.pinned,
        operator_priority: body.operator_priority,
        group_id: body.group_id,
    };
    if patch.is_empty() {
        return Err(
            "the body changes nothing; send pinned, operator_priority or group_id".to_owned(),
        );
    }
    Ok(patch)
}

fn group_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name is empty; a group needs a name".to_owned());
    }
    if name.chars().count() > GROUP_NAME_MAX_CHARS {
        return Err(format!(
            "name is longer than {GROUP_NAME_MAX_CHARS} characters"
        ));
    }
    Ok(name.to_owned())
}

fn merge_ids(root: i64, body: MergeCausesBody) -> Result<Vec<i64>, String> {
    if body.cause_ids.is_empty() {
        return Err("cause_ids is empty; name the causes to merge into this one".to_owned());
    }
    if body.cause_ids.contains(&root) {
        return Err(format!(
            "cause_ids contains {root}, the cause the others merge into"
        ));
    }
    let ids: Vec<i64> = body
        .cause_ids
        .into_iter()
        .collect::<BTreeSet<i64>>()
        .into_iter()
        .collect();
    if ids.len() > MERGE_MAX_CAUSES {
        return Err(format!(
            "cause_ids names {} causes; at most {MERGE_MAX_CAUSES} merge at once",
            ids.len()
        ));
    }
    Ok(ids)
}

fn persist(state: &AppState) -> Result<&PersistHandle, Response> {
    state.persist().ok_or_else(persist_unavailable_response)
}

fn no_cause(id: i64) -> Response {
    refuse(
        StatusCode::NOT_FOUND,
        &format!("no cause {id} is open in the triage queue"),
    )
}

pub(crate) async fn patch_cause_as_operator(
    state: State<AppState>,
    id: Path<i64>,
    headers: HeaderMap,
    body: Json<PatchCauseBody>,
) -> Result<Json<Value>, Response> {
    super::conversation::require_operator(&headers)?;
    patch_cause(state, id, body).await
}

pub(crate) async fn patch_cause(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PatchCauseBody>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let patch = cause_patch(body).map_err(bad_request)?;
    match handle
        .web()
        .patch_wright_cause(id, &patch)
        .await
        .map_err(persist_error_response)?
    {
        PatchOutcome::Updated(cause) => Ok(Json(json!({"cause": cause}))),
        PatchOutcome::NoCause => Err(no_cause(id)),
        PatchOutcome::NoGroup => Err(refuse(
            StatusCode::NOT_FOUND,
            "group_id names no group; create the group first",
        )),
    }
}

pub(crate) async fn merge_causes_as_operator(
    state: State<AppState>,
    id: Path<i64>,
    headers: HeaderMap,
    body: Json<MergeCausesBody>,
) -> Result<Json<Value>, Response> {
    super::conversation::require_operator(&headers)?;
    merge_causes(state, id, body).await
}

pub(crate) async fn merge_causes(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MergeCausesBody>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let ids = merge_ids(id, body).map_err(bad_request)?;
    match handle
        .web()
        .merge_wright_causes_by_operator(id, &ids)
        .await
        .map_err(persist_error_response)?
    {
        MergeOutcome::Merged(merged) => Ok(Json(json!({
            "causes": merged.causes,
            "problems": merged.problems,
            "merge_ids": merged.merge_ids,
        }))),
        MergeOutcome::NoRoot => Err(refuse(
            StatusCode::NOT_FOUND,
            &format!("no cause {id} exists, or it was merged into another cause"),
        )),
        MergeOutcome::Unknown(unknown) => Err(bad_request(format!(
            "cause_ids names causes that do not exist or were merged into another cause: {unknown:?}"
        ))),
    }
}

pub(crate) async fn separate_merge_as_operator(
    state: State<AppState>,
    id: Path<i64>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    super::conversation::require_operator(&headers)?;
    separate_merge(state, id).await
}

pub(crate) async fn separate_merge(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    match handle
        .web()
        .separate_wright_cause_merge(id)
        .await
        .map_err(persist_error_response)?
    {
        SeparateOutcome::Separated(merge) => Ok(Json(json!({"merge": merge}))),
        SeparateOutcome::NotFound => Err(refuse(
            StatusCode::NOT_FOUND,
            &format!("no cause merge has id {id}"),
        )),
        SeparateOutcome::EngineerMerge => Err(conflict(
            "the engineer made this merge, because a finding named the causes as one fault; only a merge the operator made can be separated",
        )),
        SeparateOutcome::AlreadySeparated => Err(conflict("this merge was already separated")),
    }
}

pub(crate) async fn list_cause_problems(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let problems = handle
        .web()
        .wright_cause_problems(id)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| refuse(StatusCode::NOT_FOUND, &format!("no cause has id {id}")))?;
    Ok(Json(json!({"problems": problems})))
}

pub(crate) async fn list_cause_groups(
    State(state): State<AppState>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let groups = handle
        .web()
        .list_wright_cause_groups()
        .await
        .map_err(persist_error_response)?;
    Ok(Json(json!({"groups": groups})))
}

fn group_response(write: GroupWrite, name: &str) -> Result<Json<Value>, Response> {
    match write {
        GroupWrite::Saved(group) => Ok(Json(json!({"group": group}))),
        GroupWrite::NameTaken => Err(conflict(&format!("a group named `{name}` already exists"))),
        GroupWrite::NoGroup => Err(refuse(StatusCode::NOT_FOUND, "no group has that id")),
    }
}

pub(crate) async fn create_cause_group_as_operator(
    state: State<AppState>,
    headers: HeaderMap,
    body: Json<GroupBody>,
) -> Result<Json<Value>, Response> {
    super::conversation::require_operator(&headers)?;
    create_cause_group(state, body).await
}

pub(crate) async fn create_cause_group(
    State(state): State<AppState>,
    Json(body): Json<GroupBody>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let name = group_name(&body.name).map_err(bad_request)?;
    let write = handle
        .web()
        .create_wright_cause_group(&name)
        .await
        .map_err(persist_error_response)?;
    group_response(write, &name)
}

pub(crate) async fn rename_cause_group_as_operator(
    state: State<AppState>,
    id: Path<i64>,
    headers: HeaderMap,
    body: Json<GroupBody>,
) -> Result<Json<Value>, Response> {
    super::conversation::require_operator(&headers)?;
    rename_cause_group(state, id, body).await
}

pub(crate) async fn rename_cause_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<GroupBody>,
) -> Result<Json<Value>, Response> {
    let handle = persist(&state)?;
    let name = group_name(&body.name).map_err(bad_request)?;
    let write = handle
        .web()
        .rename_wright_cause_group(id, &name)
        .await
        .map_err(persist_error_response)?;
    group_response(write, &name)
}

pub(crate) async fn delete_cause_group_as_operator(
    state: State<AppState>,
    id: Path<i64>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    super::conversation::require_operator(&headers)?;
    delete_cause_group(state, id).await
}

pub(crate) async fn delete_cause_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let handle = persist(&state)?;
    let deleted = handle
        .web()
        .delete_wright_cause_group(id)
        .await
        .map_err(persist_error_response)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(refuse(StatusCode::NOT_FOUND, "no group has that id"))
    }
}
