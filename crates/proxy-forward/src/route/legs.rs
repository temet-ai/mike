use bytes::Bytes;
use http::Request;
use http::Response;
use http::uri::Authority;
use http_body_util::BodyExt;

use super::ResponseBody;
use super::rebuild::rebuild;
use super::responses::bad_gateway;
use crate::upstream::{ForwardBody, UpstreamClients};

pub(super) enum RoutedFailure {
    Replayable(String),
    AfterSend(String),
}

pub(super) async fn send_routed(
    buffered: &Request<Bytes>,
    clients: &UpstreamClients,
    upstream_authority: &Authority,
) -> Result<Response<ResponseBody>, RoutedFailure> {
    let routed_req = rebuild(buffered, "http", upstream_authority.as_str(), None)
        .map_err(RoutedFailure::Replayable)?;

    match clients.routed.request(routed_req).await {
        Ok(resp) => Ok(resp.map(|b| b.boxed())),
        Err(e) if e.is_connect() => Err(RoutedFailure::Replayable(e.to_string())),
        Err(e) => Err(RoutedFailure::AfterSend(e.to_string())),
    }
}

pub(super) async fn send_direct(
    buffered: &Request<Bytes>,
    clients: &UpstreamClients,
    intercept_host: &str,
    direct_target: Option<&str>,
) -> Response<ResponseBody> {
    let dial_authority = direct_target.unwrap_or(intercept_host).to_string();
    let direct_req = match build_direct_request(buffered, &dial_authority, intercept_host) {
        Ok(r) => r,
        Err(msg) => return direct_build_failed(&msg),
    };

    match clients.direct.request(direct_req).await {
        Ok(resp) => resp.map(|b| b.boxed()),
        Err(e) => direct_leg_failed(&e),
    }
}

fn build_direct_request(
    buffered: &Request<Bytes>,
    dial_authority: &str,
    host_header: &str,
) -> Result<Request<ForwardBody>, String> {
    rebuild(buffered, "https", dial_authority, Some(host_header))
}

fn direct_build_failed(msg: &str) -> Response<ResponseBody> {
    tracing::warn!(error = %msg, "failed to build direct request");
    bad_gateway("failed to build direct request")
}

fn direct_leg_failed(e: &hyper_util::client::legacy::Error) -> Response<ResponseBody> {
    tracing::warn!(error = %e, "direct Anthropic leg failed");
    bad_gateway("direct upstream failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_rewrites_scheme_and_preserves_headers() -> Result<(), String> {
        let mut req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages?beta=1")
            .header("x-api-key", "secret")
            .header("anthropic-version", "2023-06-01")
            .body(Bytes::from_static(b"{}"))
            .map_err(|e| e.to_string())?;
        req.headers_mut()
            .insert(http::header::HOST, http::HeaderValue::from_static("old"));

        let out = build_direct_request(&req, "api.anthropic.com", "api.anthropic.com")?;
        assert_eq!(out.uri().scheme_str(), Some("https"));
        assert_eq!(
            out.uri().authority().map(|a| a.as_str()),
            Some("api.anthropic.com")
        );
        assert_eq!(out.uri().path(), "/v1/messages");
        assert_eq!(out.uri().query(), Some("beta=1"));
        assert_eq!(header(&out, "x-api-key")?, "secret");
        assert_eq!(header(&out, "host")?, "api.anthropic.com");
        Ok(())
    }

    #[test]
    fn build_direct_request_separates_dial_from_host() -> Result<(), String> {
        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .body(Bytes::from_static(b"{}"))
            .map_err(|e| e.to_string())?;
        let out = build_direct_request(&req, "127.0.0.1:54321", "api.anthropic.com")?;
        assert_eq!(
            out.uri().authority().map(|a| a.as_str()),
            Some("127.0.0.1:54321")
        );
        assert_eq!(header(&out, "host")?, "api.anthropic.com");
        Ok(())
    }

    fn header(req: &Request<ForwardBody>, name: &str) -> Result<String, String> {
        let value = req
            .headers()
            .get(name)
            .ok_or_else(|| format!("missing header {name}"))?;
        value
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| format!("header {name} not UTF-8: {e}"))
    }
}
