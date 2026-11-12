use axum::http::request::Parts;
use axum::response::Response;
use bytes::Bytes;
use serde_json::Value;

use crate::compose_prompt;
use crate::error::ProxyError;
use crate::extract::signals::RequestSignals;
use crate::metrics::{COUNTER_CAPTURE_THROTTLE_OVERRIDE_TOTAL, COUNTER_UNCAPTURED_REQUESTS_TOTAL};
use crate::state::AppState;
use crate::store::models::RequestId;
use crate::wire::manifest::ManifestBuilder;

pub(super) struct ComposeAndGuardArgs<'a> {
    pub(super) state: &'a AppState,
    pub(super) post_filter_body: Bytes,
    pub(super) body_bytes: &'a Bytes,
    pub(super) signals: &'a RequestSignals,
    pub(super) is_count_tokens_probe: bool,
    pub(super) request_id: RequestId,
    pub(super) manifest: &'a mut ManifestBuilder,
    pub(super) filter_applied: bool,
    pub(super) json_body: Option<&'a Value>,
    pub(super) parts: &'a Parts,
}

pub(super) enum ComposeAndGuardOutcome {
    Continue(compose_prompt::stage::ComposeResult),
    EarlyResponse(Response),
}

pub(super) async fn compose_and_guard(
    args: ComposeAndGuardArgs<'_>,
) -> Result<ComposeAndGuardOutcome, ProxyError> {
    let ComposeAndGuardArgs {
        state,
        post_filter_body,
        body_bytes,
        signals,
        is_count_tokens_probe,
        request_id,
        manifest,
        filter_applied,
        json_body,
        parts,
    } = args;
    let turn_origin = &signals.turn_origin;
    let subagent_persona = signals.subagent_persona.as_deref();
    let session_id = signals.session_id.as_str();

    let compose_result = compose_prompt::stage::run(
        state,
        post_filter_body,
        body_bytes,
        compose_prompt::stage::TurnIdentity {
            turn_origin,
            class_key: compose_prompt::PromptClassKey::resolve(turn_origin, subagent_persona),
            is_count_tokens_probe,
            mutation_exempt: signals.mutation_exempt,
        },
        request_id,
        manifest,
    )
    .await;
    let observed_composition_applied = compose_result.observed_composition_applied;

    verify_global_cache_scope_tripwire(CacheScopeTripwireArgs {
        json_body,
        upstream_body: &compose_result.upstream_body,
        filter_applied,
        observed_composition_applied,
        request_id,
        session_id,
    });

    if let Some(response) = evaluate_capture_admission(AdmissionCheckArgs {
        state,
        request_id,
        session_id,
        filter_applied,
        observed_composition_applied,
        parts,
        upstream_body: &compose_result.upstream_body,
    })
    .await?
    {
        return Ok(ComposeAndGuardOutcome::EarlyResponse(response));
    }

    Ok(ComposeAndGuardOutcome::Continue(compose_result))
}

pub(super) struct CacheScopeTripwireArgs<'a> {
    pub(super) json_body: Option<&'a Value>,
    pub(super) upstream_body: &'a Bytes,
    pub(super) filter_applied: bool,
    pub(super) observed_composition_applied: bool,
    pub(super) request_id: RequestId,
    pub(super) session_id: &'a str,
}

pub(super) fn verify_global_cache_scope_tripwire(args: CacheScopeTripwireArgs<'_>) {
    let CacheScopeTripwireArgs {
        json_body,
        upstream_body,
        filter_applied,
        observed_composition_applied,
        request_id,
        session_id,
    } = args;

    if !(filter_applied || observed_composition_applied) {
        return;
    }
    let Some(original_system) = json_body
        .and_then(|v| v.get("system"))
        .and_then(Value::as_array)
    else {
        return;
    };
    if crate::cache_scope::last_global_index(original_system).is_none() {
        return;
    }

    match serde_json::from_slice::<Value>(upstream_body) {
        Ok(forwarded) => {
            check_forwarded_system_preserves_global_prefix(
                original_system,
                &forwarded,
                request_id,
                session_id,
            );
        }
        Err(_) => log_forwarded_body_not_json(request_id, session_id),
    }
}

fn check_forwarded_system_preserves_global_prefix(
    original_system: &[Value],
    forwarded: &Value,
    request_id: RequestId,
    session_id: &str,
) {
    let empty = Vec::new();
    let forwarded_system = forwarded
        .get("system")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if let Err(violation) =
        crate::cache_scope::verify_global_prefix_preserved(original_system, forwarded_system)
    {
        tracing::error!(
            request_id = %request_id,
            session_id = %session_id,
            error = %violation,
            "global cache-scope prefix invariant violated by the injection pipeline"
        );
    }
}

fn log_forwarded_body_not_json(request_id: RequestId, session_id: &str) {
    tracing::error!(
        request_id = %request_id,
        session_id = %session_id,
        "global cache-scope guard: forwarded body is not JSON after a request that had a global system block"
    );
}

pub(super) struct AdmissionCheckArgs<'a> {
    pub(super) state: &'a AppState,
    pub(super) request_id: RequestId,
    pub(super) session_id: &'a str,
    pub(super) filter_applied: bool,
    pub(super) observed_composition_applied: bool,
    pub(super) parts: &'a Parts,
    pub(super) upstream_body: &'a Bytes,
}

pub(super) async fn evaluate_capture_admission(
    args: AdmissionCheckArgs<'_>,
) -> Result<Option<axum::response::Response>, ProxyError> {
    let AdmissionCheckArgs {
        state,
        request_id,
        session_id,
        filter_applied,
        observed_composition_applied,
        parts,
        upstream_body,
    } = args;

    if !state.is_capture_admission_throttled() {
        return Ok(None);
    }
    if filter_applied || observed_composition_applied {
        log_throttled_but_mutated(request_id, session_id);
        return Ok(None);
    }

    log_throttled_forwarding_without_capture(request_id, session_id);
    let response = crate::proxy::forward::forward_without_capture(
        state,
        parts.method.clone(),
        &parts.uri,
        &parts.headers,
        upstream_body.clone(),
    )
    .await?;
    Ok(Some(response))
}

fn log_throttled_but_mutated(request_id: RequestId, session_id: &str) {
    metrics::counter!(COUNTER_CAPTURE_THROTTLE_OVERRIDE_TOTAL).increment(1);
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        "cap_manager: memory pressure but request was mutated — capturing anyway",
    );
}

fn log_throttled_forwarding_without_capture(request_id: RequestId, session_id: &str) {
    metrics::counter!(COUNTER_UNCAPTURED_REQUESTS_TOTAL, "reason" => "capture_throttled")
        .increment(1);
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        "cap_manager: memory pressure — forwarding without capture",
    );
}
