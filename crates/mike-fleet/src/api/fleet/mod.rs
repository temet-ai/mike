pub mod at_ready;
pub mod changes;
pub mod check;
pub mod decide;
pub mod definition_patch;
pub mod definition_view;
pub mod definitions;
pub mod graph;
pub mod observations;
pub mod previous_runs;
pub mod propose;
pub mod record;
pub mod report;
pub mod reuse;
pub mod session_fleet;
pub mod status_poll;
pub mod tree;
pub mod verdict;

#[cfg(test)]
pub mod definition_paths_tests;
#[cfg(test)]
pub mod definition_tests;

pub use changes::get_fleet_changes;
pub use check::{check_fleet, gate_fleet, where_in_fleet};
pub use decide::decide_problem;
pub use definitions::{create_fleet, get_fleet, patch_fleet, start_fleet, stop_fleet};
pub use graph::get_fleet_graph;
pub use propose::{propose_task, split_task};
pub use record::get_fleet_record;
pub use report::report_slice;
pub use reuse::get_fleet_reuse;
pub use session_fleet::get_session_fleet;
pub use status_poll::{get_fleet_status, get_fleet_status_for_session, list_fleets};
pub use tree::{commit_fleet, get_fleet_results, integrate_fleet};
pub use verdict::record_verdict;

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
