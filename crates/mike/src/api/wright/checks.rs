use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{bad_request, refuse};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::{CheckRow, CheckRunRow, NewCheck};
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::Probe;
use crate::wright::definition::{
    merge_params, parse_cadence, parse_probe, parse_source, validate_expects, validate_name,
    validate_params, validate_query, validate_query_params,
};
use crate::wright::watch::bounded;

#[derive(Debug, Serialize)]
struct CheckView<'a> {
    #[serde(flatten)]
    check: &'a CheckRow,
    latest_run: Option<&'a CheckRunRow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddCheckBody {
    name: String,
    source: String,
    cadence: String,
    expects: String,
    probe: String,
    query: Option<String>,
    params: Option<Value>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PatchCheckBody {
    source: Option<String>,
    cadence: Option<String>,
    expects: Option<String>,
    query: Option<String>,
    params: Option<Value>,
    enabled: Option<bool>,
}

pub(crate) async fn list_checks(State(state): State<AppState>) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let checks = handle
        .web()
        .list_wright_checks()
        .await
        .map_err(persist_error_response)?;
    let runs = handle
        .web()
        .latest_wright_runs()
        .await
        .map_err(persist_error_response)?;
    let views: Vec<CheckView<'_>> = checks
        .iter()
        .map(|check| CheckView {
            check,
            latest_run: runs.iter().find(|run| run.check_name == check.name),
        })
        .collect();
    Ok(Json(json!({
        "checks": views,
        "watch_enabled": state.feature_toggle_overlay.load().wright,
    })))
}

fn new_check(body: AddCheckBody) -> Result<NewCheck, String> {
    validate_name(&body.name)?;
    parse_source(&body.source)?;
    parse_cadence(&body.cadence)?;
    validate_expects(&body.expects)?;
    let probe = parse_probe(&body.probe)?;
    validate_query(probe, body.query.as_deref())?;
    let params = body.params.unwrap_or_else(|| json!({}));
    validate_params(probe, &params)?;
    validate_reads(body.query.as_deref(), &params)?;
    Ok(NewCheck {
        name: body.name,
        source: body.source,
        cadence: body.cadence,
        expects: body.expects.trim().to_owned(),
        probe: body.probe,
        query: body.query,
        params,
        enabled: body.enabled.unwrap_or(true),
    })
}

fn validate_reads(query: Option<&str>, params: &Value) -> Result<(), String> {
    query.map_or(Ok(()), |q| validate_query_params(q, params))
}

fn patched_check(current: CheckRow, body: PatchCheckBody) -> Result<NewCheck, String> {
    let probe = parse_probe(&current.probe)?;
    let mut check = NewCheck::from(current);
    if let Some(source) = body.source {
        parse_source(&source)?;
        check.source = source;
    }
    if let Some(cadence) = body.cadence {
        parse_cadence(&cadence)?;
        check.cadence = cadence;
    }
    if let Some(expects) = body.expects {
        validate_expects(&expects)?;
        check.expects = expects.trim().to_owned();
    }
    if let Some(query) = body.query {
        validate_query(probe, Some(&query))?;
        check.query = Some(query);
    }
    if let Some(patch) = body.params {
        let params = merge_params(&check.params, &patch)?;
        validate_params(probe, &params)?;
        check.params = params;
    }
    if let Some(enabled) = body.enabled {
        check.enabled = enabled;
    }
    validate_reads(check.query.as_deref(), &check.params)?;
    Ok(check)
}

async fn refuse_failing_query(handle: &PersistHandle, check: &NewCheck) -> Result<(), Response> {
    if parse_probe(&check.probe).ok() != Some(Probe::Sql) {
        return Ok(());
    }
    let query = check.query.as_deref().unwrap_or_default();
    bounded::validate(handle.pool(), query, &check.params)
        .await
        .map_err(|e| {
            bad_request(format!(
                "the query was refused and nothing was written: {e}"
            ))
        })
}

pub(crate) async fn add_check(
    State(state): State<AppState>,
    Json(body): Json<AddCheckBody>,
) -> Result<Response, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let check = new_check(body).map_err(bad_request)?;
    refuse_failing_query(handle, &check).await?;
    let stored = handle
        .web()
        .insert_wright_check(&check)
        .await
        .map_err(persist_error_response)?;
    match stored {
        Some(row) => Ok((StatusCode::CREATED, Json(json!({"check": row}))).into_response()),
        None => Err(refuse(
            StatusCode::CONFLICT,
            &format!(
                "a check named `{}` already exists; change it with PATCH /api/wright/checks/{}",
                check.name, check.name
            ),
        )),
    }
}

fn unknown_check(name: &str) -> Response {
    refuse(
        StatusCode::NOT_FOUND,
        &format!("no Wright check is named `{name}`"),
    )
}

pub(crate) async fn patch_check(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<PatchCheckBody>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let current = handle
        .web()
        .wright_check(&name)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| unknown_check(&name))?;
    let reads_changed = body.query.is_some() || body.params.is_some();
    let check = patched_check(current, body).map_err(bad_request)?;
    if reads_changed {
        refuse_failing_query(handle, &check).await?;
    }
    let row = handle
        .web()
        .replace_wright_check(&check)
        .await
        .map_err(persist_error_response)?
        .ok_or_else(|| unknown_check(&name))?;
    Ok(Json(json!({"check": row})))
}

pub(crate) async fn delete_check(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let deleted = handle
        .web()
        .delete_wright_check(&name)
        .await
        .map_err(persist_error_response)?;
    if !deleted {
        return Err(unknown_check(&name));
    }
    Ok(Json(json!({"deleted": true, "name": name})))
}
