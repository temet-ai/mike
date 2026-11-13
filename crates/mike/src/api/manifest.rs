pub use mike_features::api::manifest::*;
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderName, Method, StatusCode, header};
use axum::middleware::from_fn_with_state;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::web_base::host_guard;

mod anthropic;
mod changes;
mod codex_observe;
mod compression_review;
mod config_history;
mod cross_session;
mod dossier;
mod eval;
mod eval_metrics;
mod evaluations;
mod filters;
mod improvement;
mod knowledge;
mod launches;
mod lesson_delivery;
mod memory;
mod projects;
mod prompt_shaping;
mod prompts;
mod proposals;
mod questions;
mod requests;
mod semantic;
mod session_agent_context;
mod session_mail;
mod session_proxy_tools;
mod session_recall;
mod session_reconcile;
mod sessions;
mod setup;
mod skill_outcomes;
mod skills;
mod steer_questions;
mod steering;
mod supplied_items;
mod verdicts;
mod websocket;
mod work_threads;
mod workspace;
mod wright;

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const INSPECT_TIMEOUT: Duration = Duration::from_secs(30);
const WAITED_TIMEOUT: Duration = Duration::from_secs(7300);

pub use crate::web_base::route_manifest::{Coverage, RouteEntry};

pub fn inspect_route_manifest() -> Vec<RouteEntry> {
    [
        requests::ROUTES,
        sessions::ROUTES,
        anthropic::ROUTES,
        memory::ROUTES,
        projects::ROUTES,
        filters::ROUTES,
        prompt_shaping::ROUTES,
        prompts::ROUTES,
        websocket::ROUTES,
        steering::ROUTES,
        workspace::ROUTES,
        launches::ROUTES,
        fleets::ROUTES,
        changes::ROUTES,
        lesson_delivery::ROUTES,
        supplied_items::ROUTES,
        session_proxy_tools::ROUTES,
        session_reconcile::ROUTES,
        knowledge::ROUTES,
        config_history::ROUTES,
        compression_review::ROUTES,
        eval::ROUTES,
        dossier::ROUTES,
        session_mail::ROUTES,
        verdicts::ROUTES,
        evaluations::ROUTES,
        eval_metrics::ROUTES,
        semantic::ROUTES,
        steer_questions::ROUTES,
        cross_session::ROUTES,
        improvement::ROUTES,
        questions::ROUTES,
        skills::ROUTES,
        skill_outcomes::ROUTES,
        codex_observe::ROUTES,
        proposals::ROUTES,
        setup::ROUTES,
        work_threads::ROUTES,
        wright::ROUTES,
        session_recall::ROUTES,
        session_agent_context::ROUTES,
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect()
}

pub fn inspect_router(state: AppState) -> Router {
    let policy = host_guard::LocalAccessPolicy::inspect(&state.config);
    local_router(state, policy)
}

pub(crate) fn local_router(state: AppState, policy: host_guard::LocalAccessPolicy) -> Router {
    let cors = build_cors_layer(&state.config.spa_origins);
    let router = requests::register(Router::new());
    let router = sessions::register(router);
    let router = anthropic::register(router);
    let router = memory::register(router);
    let router = projects::register(router);
    let router = filters::register(router);
    let router = prompt_shaping::register(router);
    let router = prompts::register(router);
    let router = steering::register(router);
    let router = workspace::register(router);
    let router = launches::register(router);
    let router = fleets::register(router);
    let router = changes::register(router);
    let router = lesson_delivery::register(router);
    let router = supplied_items::register(router);
    let router = session_proxy_tools::register(router);
    let router = session_reconcile::register(router);
    let router = knowledge::register(router);
    let router = config_history::register(router);
    let router = compression_review::register(router);
    let router = eval::register(router);
    let router = dossier::register(router);
    let router = session_mail::register(router);
    let router = verdicts::register(router);
    let router = evaluations::register(router);
    let router = eval_metrics::register(router);
    let router = semantic::register(router);
    let router = steer_questions::register(router);
    let router = cross_session::register(router);
    let router = improvement::register(router);
    let router = questions::register(router);
    let router = skills::register(router);
    let router = skill_outcomes::register(router);
    let router = codex_observe::register(router);
    let router = proposals::register(router);
    let router = setup::register(router);
    let router = work_threads::register(router);
    let router = wright::register(router);
    let router = session_recall::register(router);
    let router = session_agent_context::register(router);
    let router = websocket::register(router);
    let answered_at_once = router.route_layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        INSPECT_TIMEOUT,
    ));
    let waited = knowledge::register_waited(fleets::register_waited(Router::new()));
    let waited_for = waited.route_layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        WAITED_TIMEOUT,
    ));

    answered_at_once
        .merge(waited_for)
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(crate::api::request_span::inspect_request_span),
        )
        .layer(SetRequestIdLayer::x_request_id(
            tower_http::request_id::MakeRequestUuid,
        ))
        .layer(from_fn_with_state(policy, host_guard::guard))
        .with_state(state)
}

fn build_cors_layer(configured: &[String]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(host_guard::effective_origins(configured)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
}

#[cfg(test)]
fn concretize_path(path: &str) -> String {
    path.replace("{id}", "1")
        .replace("{project_path}", "proj")
        .replace("{prompt_sha256}", "abc123")
        .replace("{run_id}", "run-1")
        .replace("{fleet}", "fleet-1")
        .replace("{session}", "00000000-0000-0000-0000-000000000000")
        .replace("{note_uuid}", "00000000-0000-0000-0000-000000000001")
        .replace("{slug}", "thread-1")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::config::ProxyConfig;

    #[test]
    fn manifest_entries_have_unique_method_path_pairs() {
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for entry in inspect_route_manifest() {
            assert!(
                seen.insert((entry.method, entry.path)),
                "manifest has duplicate {} {}",
                entry.method,
                entry.path
            );
        }
    }

    #[test]
    fn manifest_entries_cover_all_known_methods() {
        for entry in inspect_route_manifest() {
            match entry.method {
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => {}
                other => panic!(
                    "manifest entry {} {} uses unsupported method {other}",
                    entry.method, entry.path
                ),
            }
        }
    }

    #[tokio::test]
    async fn every_manifest_entry_is_reachable_on_the_router() -> anyhow::Result<()> {
        let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let router = inspect_router(state);
        for entry in inspect_route_manifest() {
            let uri = concretize_path(entry.path);
            let req = Request::builder()
                .header("host", "127.0.0.1:9989")
                .method(Method::TRACE)
                .uri(&uri)
                .body(Body::empty())?;
            let res = router.clone().oneshot(req).await?;
            assert_eq!(
                res.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "manifest claims {} {} exists but TRACE {uri} returned {}",
                entry.method,
                entry.path,
                res.status(),
            );
        }
        Ok(())
    }

    async fn capabilities_json(router: Router) -> anyhow::Result<serde_json::Value> {
        let res = router
            .oneshot(
                Request::builder()
                    .header("host", "127.0.0.1:9989")
                    .uri("/api/capabilities")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    #[tokio::test]
    async fn scheduled_jobs_route_lists_every_job_in_the_documented_shape() -> anyhow::Result<()> {
        let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let res = inspect_router(state)
            .oneshot(
                Request::builder()
                    .header("host", "127.0.0.1:9989")
                    .uri("/api/scheduled-jobs")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(body.get("machine_uuid").is_some(), "{body}");
        assert!(body.get("machine_name").is_some(), "{body}");
        let ids: Vec<&str> = body["jobs"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("jobs must be an array: {body}"))?
            .iter()
            .filter_map(|job| job["id"].as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "learning_promote",
                "learning_audit",
                "compressor_loop",
                "cargo_sweep",
                "lessons_commit",
                "usage_audit",
                "lesson_relations",
                "nightly_tests"
            ]
        );
        for job in body["jobs"].as_array().into_iter().flatten() {
            for key in [
                "label",
                "description",
                "schedule",
                "scope",
                "owner_machine_uuid",
                "enabled",
                "state",
                "last_started_at",
                "last_finished_at",
                "last_exit_code",
                "last_output_tail",
                "next_run_at",
                "consecutive_failures",
                "detail",
                "fix",
            ] {
                assert!(job.get(key).is_some(), "job {job} missing {key}");
            }
            assert!(
                ["this_machine", "one_machine"].contains(&job["scope"].as_str().unwrap_or("")),
                "{job}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn capabilities_route_returns_expected_json_shape() -> anyhow::Result<()> {
        let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let body = capabilities_json(inspect_router(state)).await?;
        let rows = body["capabilities"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("capabilities field must be an array: {body}"))?;
        let expected_ids = [
            "steering",
            "semantic_recall",
            "weasel_intercept",
            "persistence",
            "project_knowledge",
            "rerank",
            "llm_credit",
            "remote_control_link",
            "llm_worker_lease",
            "llm_role_health",
            "worker_toggles",
            "scheduled_jobs",
            "db_cost",
            "memory_keeper",
        ];
        assert_eq!(rows.len(), expected_ids.len());
        for (row, expected_id) in rows.iter().zip(expected_ids) {
            assert_eq!(row["id"], expected_id);
            assert!(row["label"].is_string(), "row {row} missing label");
            let state = row["state"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("row {row} missing state"))?;
            assert!(["active", "degraded", "inert", "off"].contains(&state));
            assert!(row["detail"].is_string(), "row {row} missing detail");
            assert!(row["fix"].is_string(), "row {row} missing fix");
        }
        Ok(())
    }

    #[tokio::test]
    async fn capabilities_reports_persistence_off_and_steering_inert() -> anyhow::Result<()> {
        let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state
            .steerer_openrouter_key
            .store(std::sync::Arc::new(None));
        state.feature_toggle_overlay.store(std::sync::Arc::new(
            crate::feature_toggles::FeatureToggles {
                steerer: true,
                ..Default::default()
            },
        ));
        let body = capabilities_json(inspect_router(state)).await?;
        let rows = body["capabilities"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("capabilities field must be an array: {body}"))?;

        let persistence = rows
            .iter()
            .find(|row| row["id"] == "persistence")
            .ok_or_else(|| anyhow::anyhow!("missing persistence row: {body}"))?;
        assert_eq!(persistence["state"], "off", "row: {persistence}");
        assert!(
            persistence["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("ephemeral"))
        );
        assert!(!persistence["fix"].as_str().unwrap_or_default().is_empty());

        let steering = rows
            .iter()
            .find(|row| row["id"] == "steering")
            .ok_or_else(|| anyhow::anyhow!("missing steering row: {body}"))?;
        assert_eq!(steering["state"], "inert", "row: {steering}");
        assert!(
            steering["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("OpenRouter"))
        );
        assert!(!steering["fix"].as_str().unwrap_or_default().is_empty());
        Ok(())
    }
}
