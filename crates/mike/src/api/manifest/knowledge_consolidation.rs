use axum::Router;
use axum::routing::{get, post};

use crate::api::knowledge_consolidation as api;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/knowledge/consolidation/subjects",
        Coverage::Exempt {
            reason: "the /memory page lists the subjects the consolidation worker holds; an investigating session asks ask-kit's mike kit instead",
        },
    ),
    route(
        "GET",
        "/api/knowledge/consolidation/decisions",
        Coverage::Exempt {
            reason: "the /memory page shows each consolidation decision with its text before and after; an investigating session asks ask-kit's mike kit instead",
        },
    ),
    route(
        "GET",
        "/api/knowledge/consolidation/measures",
        Coverage::Exempt {
            reason: "the /memory page charts the per-project consolidation measures; an investigating session asks ask-kit's mike kit instead",
        },
    ),
    route(
        "GET",
        "/api/knowledge/consolidation/subjects/{id}",
        Coverage::Exempt {
            reason: "the /memory/subjects page lists every statement of one subject with its carrier and state; an investigating session asks ask-kit's mike kit instead",
        },
    ),
    route(
        "POST",
        "/api/knowledge/consolidation/decisions/{id}/settle",
        Coverage::Exempt {
            reason: "the operator keeps one side of an open conflict from /memory; sessions do not settle conflicts through consolidation",
        },
    ),
    route(
        "POST",
        "/api/knowledge/consolidation/decisions/{id}/undo",
        Coverage::Exempt {
            reason: "the operator undoes a consolidation decision from /memory; sessions do not reverse them",
        },
    ),
    route(
        "POST",
        "/api/knowledge/consolidation/decisions/{id}/apply",
        Coverage::Exempt {
            reason: "the operator applies a proposed file edit from /memory; sessions do not edit guidance files through consolidation",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/knowledge/consolidation/subjects", get(api::list_subjects))
        .route("/api/knowledge/consolidation/decisions", get(api::list_decisions))
        .route("/api/knowledge/consolidation/measures", get(api::list_measures))
        .route("/api/knowledge/consolidation/subjects/{id}", get(api::get_subject))
        .route(
            "/api/knowledge/consolidation/decisions/{id}/settle",
            post(api::settle),
        )
        .route(
            "/api/knowledge/consolidation/decisions/{id}/undo",
            post(api::undo),
        )
        .route(
            "/api/knowledge/consolidation/decisions/{id}/apply",
            post(api::apply),
        )
}
