use axum::Router;
use axum::routing::{get, post};

use crate::api::launches;
use crate::api::launches::ledger;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

const LEDGER_REASON: &str = "launcher-internal: `mike spawn` records and reads its launch \
                             ledger through the proxy when its own database connection is \
                             unavailable; not a read or action an agent reaches for mid-session";

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "POST",
        "/api/launches/{session}/mcp",
        Coverage::Exempt {
            reason: "mike-mcp reports here that a session listed its tools, so mike can tell \
                     a seat that started without them; not something an agent calls",
        },
    ),
    route(
        "GET",
        "/api/launch-ledger",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/runs",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "GET",
        "/api/launch-ledger/runs",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/launches",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/launches/{id}/opened",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/launches/{id}/outcome",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/launches/{id}/closed",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "GET",
        "/api/launch-ledger/sessions/{session}/channel",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "POST",
        "/api/launch-ledger/sessions/{session}/channel",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route(
        "GET",
        "/api/launch-ledger/sessions/{session}/turn",
        Coverage::Exempt {
            reason: LEDGER_REASON,
        },
    ),
    route("POST", "/api/launches", Coverage::Tool("spawn_session")),
    route("GET", "/api/launches", Coverage::Tool("list_launches")),
    route(
        "GET",
        "/api/launches/{session}",
        Coverage::Exempt {
            reason: "single-launch detail is a drill-down from list_launches, which already \
                     returns full, unsliced rows; no MCP tool until a workflow needs one \
                     launch without the rest",
        },
    ),
    route(
        "DELETE",
        "/api/launches/{session}",
        Coverage::Tool("kill_session"),
    ),
    route(
        "POST",
        "/api/launches/{session}/brief",
        Coverage::Exempt {
            reason: "launcher-internal: `mike spawn` calls it once the spawned session's \
                     Remote Control stream is linked, to push the launch brief; not a read or \
                     action an agent reaches for mid-session",
        },
    ),
    route(
        "GET",
        "/api/launches/doctor",
        Coverage::Exempt {
            reason: "live terminal-backend validation for setup/doctor/settings; opens and \
                     closes a real scratch surface, not an investigation read an agent would \
                     reach for mid-session",
        },
    ),
];

fn register_ledger(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/launch-ledger", get(ledger::hello))
        .route(
            "/api/launches/{session}/mcp",
            post(launches::mcp_seen::tools_listed_by),
        )
        .route(
            "/api/launch-ledger/runs",
            get(ledger::find_run).post(ledger::upsert_run),
        )
        .route("/api/launch-ledger/launches", post(ledger::insert_launch))
        .route(
            "/api/launch-ledger/launches/{id}/opened",
            post(ledger::record_opened),
        )
        .route(
            "/api/launch-ledger/launches/{id}/outcome",
            post(ledger::record_outcome),
        )
        .route(
            "/api/launch-ledger/launches/{id}/closed",
            post(ledger::record_closed),
        )
        .route(
            "/api/launch-ledger/sessions/{session}/channel",
            get(ledger::channel_present).post(ledger::link_channel),
        )
        .route(
            "/api/launch-ledger/sessions/{session}/turn",
            get(ledger::prompt_turn_seen),
        )
}

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    register_ledger(router)
        .route(
            "/api/launches",
            get(launches::list_launches).post(launches::create_launch),
        )
        .route("/api/launches/doctor", get(launches::get_launch_doctor))
        .route(
            "/api/launches/{session}/brief",
            post(launches::push_launch_brief),
        )
        .route(
            "/api/launches/{session}",
            get(launches::get_launch).delete(launches::kill_launch),
        )
}
