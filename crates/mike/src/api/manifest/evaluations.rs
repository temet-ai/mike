use axum::Router;
use axum::routing::{get, patch};

use crate::api::{
    decisions_calibration, steering_evaluations, steering_guard, steering_guard_effect,
};
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/decisions/calibration",
        Coverage::Exempt {
            reason: "SPA decisions calibration panel on settings/steering; agents read decisions.records through the loop's thread findings",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/evaluations",
        Coverage::Tool("list_steerer_evaluations"),
    ),
    route(
        "GET",
        "/api/steering/evaluation-stats",
        Coverage::Exempt {
            reason: "SPA rollout panel; JC08 eval.rates carries the rates to MCP",
        },
    ),
    route(
        "GET",
        "/api/steering/guard",
        Coverage::Exempt {
            reason: "SPA rollout panel; agents read evaluations through list_steerer_evaluations",
        },
    ),
    route(
        "PATCH",
        "/api/steering/guard/questions/{question}",
        Coverage::Exempt {
            reason: "operator rollout control; agents must not change a steerer's rollout",
        },
    ),
    route(
        "GET",
        "/api/steering/guard/metrics",
        Coverage::Exempt {
            reason: "SPA panel and CLI; JC08 eval.rates carries rates to MCP",
        },
    ),
    route(
        "GET",
        "/api/steering/guard/effect",
        Coverage::Exempt {
            reason: "SPA rollout panel; JC08 eval.rates carries the action_effect rows to MCP",
        },
    ),
    route(
        "GET",
        "/api/improvement/guard",
        Coverage::Exempt {
            reason: "SPA 'What the judge did' guard panel; agents read evaluations through list_steerer_evaluations",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/decisions/calibration",
            get(decisions_calibration::get_calibration),
        )
        .route(
            "/api/sessions/{id}/evaluations",
            get(steering_evaluations::list_session_evaluations),
        )
        .route(
            "/api/steering/evaluation-stats",
            get(steering_evaluations::get_evaluation_stats),
        )
        .route("/api/steering/guard", get(steering_guard::get_guard))
        .route(
            "/api/steering/guard/questions/{question}",
            patch(steering_guard::patch_guard_question),
        )
        .route(
            "/api/steering/guard/metrics",
            get(steering_guard::get_guard_metrics),
        )
        .route(
            "/api/steering/guard/effect",
            get(steering_guard_effect::get_guard_effect),
        )
        .route(
            "/api/improvement/guard",
            get(steering_guard_effect::get_improvement_guard),
        )
}
