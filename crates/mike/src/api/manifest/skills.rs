use axum::Router;
use axum::routing::{get, patch, post};

use super::{Coverage, RouteEntry, route};
use crate::api::skills::{installs, reads, writes};
use crate::config::AppState;

const PAGE: Coverage = Coverage::Exempt {
    reason: "page data for the Skills page, where a user reviews the skills mike writes into their projects",
};
const DECISION: Coverage = Coverage::Exempt {
    reason: "a user's decision on the Skills page; agents must not approve, edit or retire the skills written into a project",
};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/skills/projects", PAGE),
    route("GET", "/api/skills", PAGE),
    route("GET", "/api/skills/drafts/{id}", PAGE),
    route("POST", "/api/skills/drafts/{id}/review", DECISION),
    route("PATCH", "/api/skills/drafts/{id}/content", DECISION),
    route("POST", "/api/skills/policy", DECISION),
    route("POST", "/api/skills/retire", DECISION),
    route("POST", "/api/skills/restore", DECISION),
    route(
        "POST",
        "/api/skills/installs",
        Coverage::Exempt {
            reason: "called by the learning promoter at the end of each run to record what it wrote on this machine",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/skills/projects", get(reads::list_projects))
        .route("/api/skills", get(reads::list_folders))
        .route("/api/skills/drafts/{id}", get(reads::get_draft))
        .route("/api/skills/drafts/{id}/review", post(writes::review_draft))
        .route("/api/skills/drafts/{id}/content", patch(writes::edit_draft))
        .route("/api/skills/policy", post(writes::set_policy))
        .route("/api/skills/retire", post(writes::retire_folder))
        .route("/api/skills/restore", post(writes::restore_folder))
        .route("/api/skills/installs", post(installs::report_installs))
}
