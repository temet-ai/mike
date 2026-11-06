mod bypass;
mod decision;
mod legs;
mod rebuild;
mod responses;
mod upgrade;

use bytes::Bytes;
use http::uri::Authority;
use http::{Request, Response};
use http_body_util::{BodyExt, Limited};
use hyper::body::Incoming;
use tokio_util::task::TaskTracker;

pub use decision::{Leg, decide};
pub use responses::ResponseBody;

use decision::is_versioned_api_path;
use legs::RoutedFailure;
use responses::{bad_gateway, payload_too_large};
use upgrade::{dispatch_upgrade, is_upgrade_request};

use crate::health::{HealthState, ProxyHealth};
use crate::upstream::UpstreamClients;

pub struct DispatchCtx<'a> {
    pub health: &'a ProxyHealth,
    pub clients: &'a UpstreamClients,
    pub upstream_authority: &'a Authority,
    pub intercept_host: &'a str,
    pub direct_target: Option<&'a str>,
    pub max_body_bytes: usize,
    pub tracker: &'a TaskTracker,
}

pub async fn dispatch(req: Request<Incoming>, ctx: DispatchCtx<'_>) -> Response<ResponseBody> {
    if is_upgrade_request(&req) {
        tracing::info!(
            method = %req.method(),
            path = %req.uri().path(),
            "HTTP upgrade (WebSocket) → Direct bridge, NOT captured"
        );
        return dispatch_upgrade(
            req,
            ctx.clients,
            ctx.intercept_host,
            ctx.direct_target,
            ctx.tracker,
        )
        .await;
    }

    let path = req.uri().path().to_string();
    let is_event_stream = is_event_stream_request(&req);
    let health_up = ctx.health.is_up();
    let leg = leg_for(health_up, &path, is_event_stream);
    log_route_decision(leg, &path, is_event_stream, health_up);

    let buffered = match buffer_request(req, ctx.max_body_bytes).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    dispatch_on_leg(leg, buffered, &ctx, is_event_stream).await
}

fn leg_for(health_up: bool, path: &str, is_event_stream: bool) -> Leg {
    if is_event_stream && is_versioned_api_path(path) {
        return Leg::Routed;
    }
    decide(health_up, path)
}

fn is_event_stream_request<B>(req: &Request<B>) -> bool {
    req.method() == http::Method::GET
        && req
            .headers()
            .get(http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/event-stream"))
}

fn log_route_decision(leg: Leg, path: &str, is_event_stream: bool, health_up: bool) {
    match leg {
        Leg::Routed if is_event_stream => log_routed_event_stream(path),
        Leg::Routed => log_routed_captured(path),
        Leg::Direct if is_versioned_api_path(path) => {
            log_direct_uncaptured_versioned(path, health_up)
        }
        Leg::Direct => log_direct_uncaptured_non_api(path),
    }
}

fn log_routed_event_stream(path: &str) {
    tracing::info!(%path, "→ routed to :9988 (long-lived event stream, relayed as written)");
}

fn log_routed_captured(path: &str) {
    tracing::debug!(%path, "→ routed to :9988 (captured)");
}

fn log_direct_uncaptured_versioned(path: &str, health_up: bool) {
    tracing::warn!(%path, health_up, "→ Direct to upstream (NOT captured — versioned API path skipped because the routed leg is down/unknown)");
}

fn log_direct_uncaptured_non_api(path: &str) {
    tracing::info!(%path, "→ Direct to upstream (NOT captured — non-versioned-API path on the MITM leg)");
}

async fn buffer_request(
    req: Request<Incoming>,
    max_body_bytes: usize,
) -> Result<Request<Bytes>, Response<ResponseBody>> {
    let (parts, body) = req.into_parts();
    let collected = match Limited::new(body, max_body_bytes).collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            tracing::warn!(
                error = %e,
                max_body_bytes,
                "request body exceeded the max buffered size (or failed to read); rejecting 413"
            );
            return Err(payload_too_large(max_body_bytes));
        }
    };
    Ok(Request::from_parts(parts, collected))
}

async fn dispatch_on_leg(
    leg: Leg,
    buffered: Request<Bytes>,
    ctx: &DispatchCtx<'_>,
    is_event_stream: bool,
) -> Response<ResponseBody> {
    match leg {
        Leg::Routed => dispatch_routed(buffered, ctx, is_event_stream).await,
        Leg::Direct => {
            let reason = bypass::reason_for_direct_leg(buffered.uri().path());
            bypass::send_direct_logged(&buffered, ctx, reason).await
        }
    }
}

async fn dispatch_routed(
    buffered: Request<Bytes>,
    ctx: &DispatchCtx<'_>,
    is_event_stream: bool,
) -> Response<ResponseBody> {
    match legs::send_routed(&buffered, ctx.clients, ctx.upstream_authority).await {
        Ok(resp) => resp,
        Err(RoutedFailure::Replayable(msg)) if is_event_stream => {
            event_stream_replay_skipped(ctx, &msg)
        }
        Err(RoutedFailure::Replayable(msg)) => {
            retry_direct_after_replayable_failure(&buffered, ctx, &msg).await
        }
        Err(RoutedFailure::AfterSend(msg)) => mark_down_after_send_failure(ctx, &msg),
    }
}

async fn retry_direct_after_replayable_failure(
    buffered: &Request<Bytes>,
    ctx: &DispatchCtx<'_>,
    msg: &str,
) -> Response<ResponseBody> {
    ctx.health.set(HealthState::Down);
    tracing::warn!(reason = %msg, "routed leg unusable pre-send; retrying Direct, marking Down");
    bypass::send_direct_logged(buffered, ctx, bypass::BypassReason::RoutedLegFailedPreSend).await
}

fn event_stream_replay_skipped(ctx: &DispatchCtx<'_>, msg: &str) -> Response<ResponseBody> {
    ctx.health.set(HealthState::Down);
    tracing::warn!(
        reason = %msg,
        "routed leg unusable pre-send for an event-stream request; skipping Direct replay \
         (would permanently bypass capture), marking Down, returning 502 for client reconnect"
    );
    bad_gateway("routed upstream unavailable for event-stream request")
}

fn mark_down_after_send_failure(ctx: &DispatchCtx<'_>, msg: &str) -> Response<ResponseBody> {
    ctx.health.set(HealthState::Down);
    tracing::warn!(error = %msg, "routed leg failed after send; returning 502");
    bad_gateway("routed upstream failed mid-request")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, accept: Option<&str>) -> Result<Request<()>, http::Error> {
        let mut builder = Request::builder()
            .method(method)
            .uri("/v1/code/sessions/s/worker/events/stream");
        if let Some(accept) = accept {
            builder = builder.header(http::header::ACCEPT, accept);
        }
        builder.body(())
    }

    #[test]
    fn leg_for_event_stream_routes_even_when_health_is_down() {
        let path = "/v1/code/sessions/s/worker/events/stream";
        assert_eq!(leg_for(false, path, true), Leg::Routed);
        assert_eq!(leg_for(true, path, true), Leg::Routed);
    }

    #[test]
    fn leg_for_non_event_stream_follows_health() {
        assert_eq!(leg_for(false, "/v1/messages", false), Leg::Direct);
        assert_eq!(leg_for(true, "/v1/messages", false), Leg::Routed);
        assert_eq!(leg_for(false, "/health", true), Leg::Direct);
    }

    #[test]
    fn is_event_stream_request_get_with_event_stream_accept_is_true() -> Result<(), http::Error> {
        assert!(is_event_stream_request(&request(
            "GET",
            Some("text/event-stream")
        )?));
        Ok(())
    }

    #[test]
    fn is_event_stream_request_post_with_event_stream_accept_is_false() -> Result<(), http::Error> {
        assert!(!is_event_stream_request(&request(
            "POST",
            Some("text/event-stream")
        )?));
        Ok(())
    }

    #[test]
    fn is_event_stream_request_get_with_json_accept_is_false() -> Result<(), http::Error> {
        assert!(!is_event_stream_request(&request(
            "GET",
            Some("application/json")
        )?));
        Ok(())
    }

    #[test]
    fn is_event_stream_request_get_with_no_accept_header_is_false() -> Result<(), http::Error> {
        assert!(!is_event_stream_request(&request("GET", None)?));
        Ok(())
    }

    fn dummy_trust_root() -> anyhow::Result<rustls_pki_types::CertificateDer<'static>> {
        let key = rcgen::KeyPair::generate()?;
        let mut params = rcgen::CertificateParams::default();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let cert = params.self_signed(&key)?;
        Ok(cert.der().clone())
    }

    #[tokio::test]
    async fn dispatch_routed_event_stream_replayable_failure_marks_health_down_and_returns_502()
    -> anyhow::Result<()> {
        let health = ProxyHealth::unknown();
        health.set(HealthState::Up);

        let root = dummy_trust_root()?;
        let clients = UpstreamClients::build(std::slice::from_ref(&root), None)
            .map_err(|e| anyhow::anyhow!(e))?;
        let upstream_authority: Authority = "127.0.0.1:1".parse()?;
        let tracker = TaskTracker::new();
        let ctx = DispatchCtx {
            health: &health,
            clients: &clients,
            upstream_authority: &upstream_authority,
            intercept_host: "api.anthropic.com",
            direct_target: None,
            max_body_bytes: 1024,
            tracker: &tracker,
        };

        let buffered = Request::builder()
            .method("GET")
            .uri("http://api.anthropic.com/v1/code/sessions/s/worker/events/stream")
            .header(http::header::ACCEPT, "text/event-stream")
            .body(Bytes::new())?;

        let resp = dispatch_routed(buffered, &ctx, true).await;

        assert_eq!(resp.status(), http::StatusCode::BAD_GATEWAY);
        assert_eq!(
            health.get(),
            HealthState::Down,
            "a Replayable failure must mark the routed leg Down even when the \
             Direct replay is skipped"
        );
        Ok(())
    }
}
