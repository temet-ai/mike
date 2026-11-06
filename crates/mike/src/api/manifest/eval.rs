use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};

use crate::api::eval::{bundles, labels, rejudge_runs, relay, review, runs, session_end_items};
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/eval/bundles",
        Coverage::Tool("list_eval_bundles"),
    ),
    route(
        "GET",
        "/api/eval/bundles/{id}",
        Coverage::AggregatedInto("list_eval_bundles"),
    ),
    route(
        "GET",
        "/api/eval/vocabulary",
        Coverage::Exempt {
            reason: "label vocabulary for the SPA picker; relay_label's schema carries the same enums",
        },
    ),
    route(
        "GET",
        "/api/eval/labels",
        Coverage::Tool("list_eval_labels"),
    ),
    route(
        "POST",
        "/api/eval/labels",
        Coverage::Exempt {
            reason: GOLD_REASON,
        },
    ),
    route(
        "POST",
        "/api/eval/labels/{id}/withdraw",
        Coverage::Exempt {
            reason: GOLD_REASON,
        },
    ),
    route(
        "POST",
        "/api/eval/labels/{id}/confirm",
        Coverage::Exempt {
            reason: GOLD_REASON,
        },
    ),
    route(
        "POST",
        "/api/eval/labels/relay",
        Coverage::Tool("relay_label"),
    ),
    route("GET", "/api/eval/runs", Coverage::Tool("list_eval_runs")),
    route("GET", "/api/eval/runs/{id}", Coverage::Tool("get_eval_run")),
    route(
        "GET",
        "/api/eval/runs/{id}/items",
        Coverage::Tool("list_eval_run_items"),
    ),
    route(
        "GET",
        "/api/eval/runs/{id}/items/{turn_id}/review",
        Coverage::Exempt {
            reason: "evidence pane of the SPA eval review page; list_eval_run_items and get_session_messages already reach the same turn",
        },
    ),
    route("GET", "/api/eval/gates", Coverage::Tool("list_eval_gates")),
    route(
        "GET",
        "/api/eval/gates/{id}",
        Coverage::Tool("get_eval_gate"),
    ),
    route(
        "GET",
        "/api/eval/import-batches",
        Coverage::Exempt {
            reason: "import audit for the operator; mike eval import prints the same manifest",
        },
    ),
    route(
        "GET",
        "/api/eval/rejudge-runs",
        Coverage::Exempt {
            reason: REJUDGE_REASON,
        },
    ),
    route(
        "GET",
        "/api/eval/rejudge-runs/{id}",
        Coverage::Exempt {
            reason: REJUDGE_REASON,
        },
    ),
    route(
        "GET",
        "/api/eval/runs/{id}/session-end-items",
        Coverage::Tool("list_eval_session_end_items"),
    ),
];

const REJUDGE_REASON: &str = "operator spend control for the one-off D9 re-judge; started and cancelled from the mike CLI, read on /settings/workers";

const GOLD_REASON: &str = "gold labels are operator ground truth, written only from the SPA control (decision D2); agents relay the operator's words through relay_label, which writes the non-gold relayed tier";

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/eval/bundles", get(bundles::list_eval_bundles))
        .route("/api/eval/bundles/{id}", get(bundles::get_eval_bundle))
        .route("/api/eval/vocabulary", get(labels::get_vocabulary))
        .route(
            "/api/eval/labels",
            get(labels::list_eval_labels).post(labels::post_gold_labels),
        )
        .route("/api/eval/labels/relay", post(relay::post_relay_label))
        .route(
            "/api/eval/labels/{id}/withdraw",
            post(labels::withdraw_label),
        )
        .route("/api/eval/labels/{id}/confirm", post(labels::confirm_label))
        .route("/api/eval/import-batches", get(labels::list_import_batches))
        .route("/api/eval/runs", get(runs::list_eval_runs))
        .route("/api/eval/runs/{id}", get(runs::get_eval_run))
        .route("/api/eval/runs/{id}/items", get(runs::list_eval_run_items))
        .route(
            "/api/eval/runs/{id}/items/{turn_id}/review",
            get(review::get_eval_run_item_review),
        )
        .route(
            "/api/eval/runs/{id}/session-end-items",
            get(session_end_items::list_session_end_items),
        )
        .route("/api/eval/gates", get(runs::list_eval_gates))
        .route("/api/eval/gates/{id}", get(runs::get_eval_gate))
        .route(
            "/api/eval/rejudge-runs",
            get(rejudge_runs::list_rejudge_runs),
        )
        .route(
            "/api/eval/rejudge-runs/{id}",
            get(rejudge_runs::get_rejudge_run),
        )
}
