use axum::Router;
use axum::routing::{get, patch, post, put};

use super::{Coverage, RouteEntry, route};
use crate::api::handlers;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/sessions/{id}/steering",
        Coverage::Exempt {
            reason: "Phase 19 steering: per-session intervention timeline; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/enrolments",
        Coverage::Exempt {
            reason: "Phase 19 steering: per-session enrolment badge; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/filter-rules",
        Coverage::Exempt {
            reason: "Session controls filter-rule inventory: real per-session firing counts; SPA-driven, not surfaced via MCP per demand-driven policy",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/verifier-counts",
        Coverage::Exempt {
            reason: "Phase C session-controls: per-session Verifier fired/delivered counts; SPA-driven, not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/validation-status",
        Coverage::Exempt {
            reason: "issue temet-ai/mike-private#117 visible validation state: latest verifier evaluation attempt for the session's live 'being validated' indicator; SPA-driven, not surfaced via MCP per demand-driven policy",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/analyzers",
        Coverage::Exempt {
            reason: "Session controls analyzer inventory: real per-session hit counts + live enable state; SPA-driven, not surfaced via MCP per demand-driven policy",
        },
    ),
    route(
        "PATCH",
        "/api/sessions/{id}/analyzers/{name}",
        Coverage::Exempt {
            reason: "Session controls analyzer inventory: per-session analyzer enable/disable override; SPA-driven, not surfaced via MCP per demand-driven policy",
        },
    ),
    route(
        "PUT",
        "/api/sessions/{id}/goal",
        Coverage::Tool("set_session_goal"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/goal",
        Coverage::Exempt {
            reason: "Session goal read-back; agents write via set_session_goal MCP tool; direct GET is SPA / debug only",
        },
    ),
    route(
        "GET",
        "/api/steering/vocabulary",
        Coverage::Exempt {
            reason: "Steering vocabulary lookup tables; SPA-driven tooltips and help text, not surfaced via MCP V1",
        },
    ),
    route(
        "GET",
        "/api/models",
        Coverage::Exempt {
            reason: "Phase 13: OpenRouter model catalogue; SPA-only model picker, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/llm/providers",
        Coverage::Exempt {
            reason: "Provider and model pickers: which of Claude, Codex and OpenRouter this machine can use, and each one's model list; SPA-only, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/usage/models",
        Coverage::Exempt {
            reason: "Usage settings page: per-model request/token aggregate; SPA-only, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/usage/openrouter",
        Coverage::Exempt {
            reason: "Usage settings page: OpenRouter account usage/credits; SPA-only, not needed via MCP",
        },
    ),
    route(
        "POST",
        "/api/steerers/assist",
        Coverage::Exempt {
            reason: "Phase 13: AI-powered steerer config suggestion; SPA-only feature, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/steerers/dry-run",
        Coverage::Exempt {
            reason: "dry-run verdict evaluation; SPA-only feature, not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/steerers",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: steerer list; SPA-driven fleet management, not surfaced via MCP V1",
        },
    ),
    route(
        "POST",
        "/api/steerers",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: steerer create; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "PUT",
        "/api/steerers/model",
        Coverage::Exempt {
            reason: "bulk steerer model set; SPA-driven fleet management, not surfaced via MCP V1",
        },
    ),
    route(
        "GET",
        "/api/steerers/{id}",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: steerer detail; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "PATCH",
        "/api/steerers/{id}",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: steerer patch; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "DELETE",
        "/api/steerers/{id}",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: steerer soft-delete; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/enrolments",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: enrol session in steerer; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "DELETE",
        "/api/sessions/{id}/enrolments/{enrolment_id}",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: disenrol session from steerer; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "PATCH",
        "/api/interventions/{id}/revert",
        Coverage::Exempt {
            reason: "Phase 19 Phase C: revert a fired intervention; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "GET",
        "/api/settings/features",
        Coverage::Exempt {
            reason: "Phase 18 settings: feature toggle list; SPA-driven settings page, not needed via MCP",
        },
    ),
    route(
        "PATCH",
        "/api/settings/features/{key}",
        Coverage::Exempt {
            reason: "Phase 18 settings: feature toggle update; SPA-driven settings page, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/settings",
        Coverage::Exempt {
            reason: "Phase 19 settings: generic settings catalogue; SPA-driven settings page, not needed via MCP",
        },
    ),
    route(
        "PATCH",
        "/api/settings/{key}",
        Coverage::Exempt {
            reason: "Phase 19 settings: generic setting update; SPA-driven settings page, not needed via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/steerer/openrouter/verify",
        Coverage::Exempt {
            reason: "Phase 13 steerer: env-var key verification; SPA-triggered check, not surfaced via MCP",
        },
    ),
    route(
        "PUT",
        "/api/settings/steerer/openrouter/key",
        Coverage::Exempt {
            reason: "steerer: runtime OpenRouter key set/verify; SPA-triggered mutation, not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/steering/verifier-rate",
        Coverage::Tool("get_verifier_rate"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    let router = register_session_views(router);
    let router = register_management(router);
    register_settings(router)
}

fn register_session_views(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/steering/vocabulary",
            get(handlers::get_steering_vocabulary),
        )
        .route(
            "/api/sessions/{id}/steering",
            get(handlers::get_session_steering),
        )
        .route(
            "/api/sessions/{id}/enrolments",
            get(handlers::get_session_enrolments).post(handlers::enrol_session),
        )
        .route(
            "/api/sessions/{id}/filter-rules",
            get(handlers::get_session_filter_rules),
        )
        .route(
            "/api/sessions/{id}/verifier-counts",
            get(handlers::get_session_verifier_counts),
        )
        .route(
            "/api/sessions/{id}/validation-status",
            get(handlers::get_session_validation_status),
        )
        .route(
            "/api/sessions/{id}/enrolments/{enrolment_id}",
            axum::routing::delete(handlers::disenrol_session),
        )
        .route(
            "/api/sessions/{id}/analyzers",
            get(handlers::get_session_analyzers),
        )
        .route(
            "/api/sessions/{id}/analyzers/{name}",
            patch(handlers::patch_session_analyzer),
        )
}

fn register_management(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/models", get(handlers::get_models))
        .route("/api/llm/providers", get(handlers::get_llm_providers))
        .route("/api/usage/models", get(handlers::get_model_usage))
        .route("/api/usage/openrouter", get(handlers::get_openrouter_usage))
        .route("/api/steerers/assist", post(handlers::steerer_assist))
        .route("/api/steerers/dry-run", post(handlers::steerer_dry_run))
        .route(
            "/api/steerers",
            get(handlers::list_steerers).post(handlers::create_steerer),
        )
        .route("/api/steerers/model", put(handlers::set_all_steerer_models))
        .route(
            "/api/steerers/{id}",
            get(handlers::get_steerer)
                .patch(handlers::patch_steerer)
                .delete(handlers::delete_steerer),
        )
        .route(
            "/api/interventions/{id}/revert",
            patch(handlers::revert_intervention),
        )
}

fn register_settings(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/settings/features", get(handlers::list_features))
        .route(
            "/api/settings/features/{key}",
            patch(handlers::patch_feature),
        )
        .route("/api/settings", get(handlers::list_settings))
        .route(
            "/api/settings/steerer/openrouter/verify",
            post(handlers::verify_steerer_openrouter_key),
        )
        .route(
            "/api/settings/steerer/openrouter/key",
            put(handlers::put_steerer_openrouter_key),
        )
        .route("/api/settings/{key}", patch(handlers::patch_setting))
        .route(
            "/api/steering/verifier-rate",
            get(handlers::get_verifier_rate),
        )
}
