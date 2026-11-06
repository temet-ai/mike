use http::{Request, Response, StatusCode, Uri, Version};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio_util::task::TaskTracker;

use super::ResponseBody;
use super::responses::{bad_gateway, empty_body};
use crate::upstream::{ForwardBody, UpstreamClients};

pub(super) fn is_upgrade_request<B>(req: &Request<B>) -> bool {
    if req.version() != Version::HTTP_11 {
        return false;
    }
    if req.headers().get(http::header::UPGRADE).is_none() {
        return false;
    }
    req.headers()
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
}

pub(super) async fn dispatch_upgrade(
    mut req: Request<Incoming>,
    clients: &UpstreamClients,
    intercept_host: &str,
    direct_target: Option<&str>,
    tracker: &TaskTracker,
) -> Response<ResponseBody> {
    let on_upgrade_client = hyper::upgrade::on(&mut req);

    let dial_authority = direct_target.unwrap_or(intercept_host).to_string();
    let mut upstream_resp =
        match request_upgrade_upstream(req, &dial_authority, intercept_host, clients).await {
            Ok(resp) => resp,
            Err(resp) => return resp,
        };

    if upstream_resp.status() != StatusCode::SWITCHING_PROTOCOLS {
        return upstream_resp.map(|b| b.boxed());
    }

    let on_upgrade_upstream = hyper::upgrade::on(&mut upstream_resp);
    let mirrored = mirror_switching_protocols(&upstream_resp);

    tracker.spawn(bridge_upgrade(on_upgrade_client, on_upgrade_upstream));

    mirrored
}

async fn request_upgrade_upstream(
    req: Request<Incoming>,
    dial_authority: &str,
    intercept_host: &str,
    clients: &UpstreamClients,
) -> Result<Response<Incoming>, Response<ResponseBody>> {
    let direct_req = match rebuild_upgrade(req, dial_authority, intercept_host) {
        Ok(r) => r,
        Err(msg) => {
            tracing::warn!(error = %msg, "failed to build direct upgrade request");
            return Err(bad_gateway("failed to build direct upgrade request"));
        }
    };

    clients.direct.request(direct_req).await.map_err(|e| {
        tracing::warn!(error = %e, "direct Anthropic leg failed for upgrade request");
        bad_gateway("direct upstream failed")
    })
}

async fn bridge_upgrade(
    on_upgrade_client: hyper::upgrade::OnUpgrade,
    on_upgrade_upstream: hyper::upgrade::OnUpgrade,
) {
    let Some((client_upgraded, upstream_upgraded)) =
        await_upgrade_pair(on_upgrade_client, on_upgrade_upstream).await
    else {
        return;
    };
    relay_upgraded_streams(client_upgraded, upstream_upgraded).await;
}

async fn await_upgrade_pair(
    on_upgrade_client: hyper::upgrade::OnUpgrade,
    on_upgrade_upstream: hyper::upgrade::OnUpgrade,
) -> Option<(hyper::upgrade::Upgraded, hyper::upgrade::Upgraded)> {
    match tokio::try_join!(on_upgrade_client, on_upgrade_upstream) {
        Ok(pair) => Some(pair),
        Err(e) => {
            tracing::warn!(error = %e, "upgrade handshake did not complete; dropping bridge");
            None
        }
    }
}

async fn relay_upgraded_streams(
    client_upgraded: hyper::upgrade::Upgraded,
    upstream_upgraded: hyper::upgrade::Upgraded,
) {
    let mut client_io = TokioIo::new(client_upgraded);
    let mut upstream_io = TokioIo::new(upstream_upgraded);
    if let Err(e) = tokio::io::copy_bidirectional(&mut client_io, &mut upstream_io).await {
        tracing::debug!(error = %e, "upgraded stream bridge ended");
    }
}

fn mirror_switching_protocols(upstream_resp: &Response<Incoming>) -> Response<ResponseBody> {
    let mut builder = Response::builder().status(upstream_resp.status());
    if let Some(headers) = builder.headers_mut() {
        *headers = upstream_resp.headers().clone();
    }
    builder
        .body(empty_body())
        .unwrap_or_else(|_| Response::new(empty_body()))
}

fn rebuild_upgrade(
    req: Request<Incoming>,
    dial_authority: &str,
    host_header: &str,
) -> Result<Request<ForwardBody>, String> {
    let auth: http::uri::Authority = dial_authority
        .parse()
        .map_err(|e| format!("bad authority {dial_authority:?}: {e}"))?;

    let pq = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let new_uri = Uri::builder()
        .scheme("https")
        .authority(auth)
        .path_and_query(pq)
        .build()
        .map_err(|e| format!("build uri: {e}"))?;

    let (mut parts, body) = req.into_parts();
    parts.uri = new_uri;
    if let Ok(hv) = http::HeaderValue::from_str(host_header) {
        parts.headers.insert(http::header::HOST, hv);
    }

    Ok(Request::from_parts(parts, body.boxed()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_upgrade_request_detects_websocket_shape() -> Result<(), String> {
        let req = Request::builder()
            .method("GET")
            .uri("https://api.anthropic.com/v1/control")
            .version(Version::HTTP_11)
            .header("connection", "keep-alive, Upgrade")
            .header("upgrade", "websocket")
            .body(())
            .map_err(|e| e.to_string())?;
        assert!(is_upgrade_request(&req));
        Ok(())
    }

    #[test]
    fn is_upgrade_request_rejects_plain_request() -> Result<(), String> {
        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .version(Version::HTTP_11)
            .body(())
            .map_err(|e| e.to_string())?;
        assert!(!is_upgrade_request(&req));
        Ok(())
    }
}
