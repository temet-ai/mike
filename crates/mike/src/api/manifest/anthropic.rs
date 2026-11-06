use axum::Router;
use axum::routing::{get, post, put};

use super::{Coverage, RouteEntry, route};
use crate::api::handlers;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/settings/assistant/anthropic",
        Coverage::Exempt {
            reason: "Anthropic credentials integration status; SPA-driven settings panel, not surfaced via MCP",
        },
    ),
    route(
        "PUT",
        "/api/settings/assistant/anthropic/key",
        Coverage::Exempt {
            reason: "Anthropic credentials integration: runtime API key set/verify; SPA-triggered mutation, not surfaced via MCP",
        },
    ),
    route(
        "DELETE",
        "/api/settings/assistant/anthropic/key",
        Coverage::Exempt {
            reason: "Anthropic credentials integration: clears the in-memory API key; SPA-triggered mutation, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/assistant/anthropic/test",
        Coverage::Exempt {
            reason: "Anthropic credentials integration: connection test; SPA-triggered check, not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/settings/assistant/anthropic/credentials",
        Coverage::Exempt {
            reason: "service-only credential handoff for prompt-assistant (services/prompt-assistant); never called by the SPA — needs the x-mike-service-token header, whose value mike publishes owner-only in its state directory",
        },
    ),
    route(
        "GET",
        "/api/settings/assistant/anthropic/cli-status",
        Coverage::Exempt {
            reason: "Claude Code CLI sign-in status for the Integrations card; SPA-driven, briefly cached, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/assistant/anthropic/cli-login",
        Coverage::Exempt {
            reason: "starts the claude CLI's own login flow from mike; SPA-triggered process spawn, not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/settings/assistant/anthropic/cli-login/{id}",
        Coverage::Exempt {
            reason: "polls a claude CLI sign-in in progress; SPA-driven, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/assistant/anthropic/cli-login/{id}/code",
        Coverage::Exempt {
            reason: "writes a pasted sign-in code back to the claude CLI's stdin; SPA-triggered, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/assistant/anthropic/cli-login/{id}/cancel",
        Coverage::Exempt {
            reason: "cancels an in-progress claude CLI sign-in; SPA-triggered, not surfaced via MCP",
        },
    ),
    route(
        "POST",
        "/api/settings/assistant/anthropic/cli-logout",
        Coverage::Exempt {
            reason: "signs the machine's claude CLI out; SPA-triggered mutation, confirmed inline, not surfaced via MCP",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/settings/assistant/anthropic",
            get(handlers::get_anthropic_integration),
        )
        .route(
            "/api/settings/assistant/anthropic/key",
            put(handlers::put_anthropic_key).delete(handlers::clear_anthropic_key),
        )
        .route(
            "/api/settings/assistant/anthropic/test",
            post(handlers::test_anthropic_connection),
        )
        .route(
            "/api/settings/assistant/anthropic/credentials",
            get(handlers::get_anthropic_credentials_for_service),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-status",
            get(handlers::get_anthropic_cli_status),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-login",
            post(handlers::start_anthropic_cli_login),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-login/{id}",
            get(handlers::get_anthropic_cli_login),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-login/{id}/code",
            post(handlers::submit_anthropic_cli_login_code),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-login/{id}/cancel",
            post(handlers::cancel_anthropic_cli_login),
        )
        .route(
            "/api/settings/assistant/anthropic/cli-logout",
            post(handlers::logout_anthropic_cli),
        )
}
