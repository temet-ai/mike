mod at_ready;
mod changes;
mod check;
mod decide;
pub(crate) mod definition_patch;
mod definition_view;
mod definitions;
mod graph;
mod observations;
mod propose;
mod record;
pub(crate) mod report;
mod reuse;
mod session_fleet;
mod status_poll;
mod tree;
mod verdict;

#[cfg(test)]
mod definition_paths_tests;
#[cfg(test)]
mod definition_tests;

pub(crate) use changes::get_fleet_changes;
pub(crate) use check::{check_fleet, gate_fleet, where_in_fleet};
pub(crate) use decide::decide_problem;
pub(crate) use definitions::{create_fleet, get_fleet, patch_fleet, start_fleet, stop_fleet};
pub(crate) use graph::get_fleet_graph;
pub(crate) use propose::{propose_task, split_task};
pub(crate) use record::get_fleet_record;
pub(crate) use report::report_slice;
pub(crate) use reuse::get_fleet_reuse;
pub(crate) use session_fleet::get_session_fleet;
pub(crate) use status_poll::{get_fleet_status, get_fleet_status_for_session, list_fleets};
pub(crate) use tree::{commit_fleet, get_fleet_results, integrate_fleet};
pub(crate) use verdict::record_verdict;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

const DEFAULT_WINDOW_HOURS: i32 = 24;

#[derive(Serialize)]
struct FleetErrorBody {
    error: &'static str,
    message: String,
}

fn fleet_error(status: StatusCode, error: &'static str, message: String) -> Response {
    (status, Json(FleetErrorBody { error, message })).into_response()
}

#[cfg(test)]
mod admission_tests;
