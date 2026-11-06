use bytes::Bytes;
use http::{HeaderName, Request, Uri};
use http_body_util::{BodyExt, Full};

use crate::upstream::ForwardBody;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "proxy-connection",
];

fn is_stripped_header(name: &HeaderName, connection_listed: &[String]) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    if lower == "content-length" {
        return true;
    }
    if HOP_BY_HOP.iter().any(|h| *h == lower) {
        return true;
    }
    connection_listed.contains(&lower)
}

fn connection_listed_headers(buffered: &Request<Bytes>) -> Vec<String> {
    let mut out = Vec::new();
    for value in buffered.headers().get_all(http::header::CONNECTION) {
        if let Ok(s) = value.to_str() {
            for token in s.split(',') {
                let token = token.trim().to_ascii_lowercase();
                if !token.is_empty() {
                    out.push(token);
                }
            }
        }
    }
    out
}

pub(super) fn rebuild(
    buffered: &Request<Bytes>,
    scheme: &str,
    authority: &str,
    host_override: Option<&str>,
) -> Result<Request<ForwardBody>, String> {
    let auth: http::uri::Authority = authority
        .parse()
        .map_err(|e| format!("bad authority {authority:?}: {e}"))?;

    let pq = buffered
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let new_uri = Uri::builder()
        .scheme(scheme)
        .authority(auth)
        .path_and_query(pq.to_string())
        .build()
        .map_err(|e| format!("build uri: {e}"))?;

    let mut builder = Request::builder()
        .method(buffered.method().clone())
        .uri(new_uri);

    let connection_listed = connection_listed_headers(buffered);

    if let Some(headers) = builder.headers_mut() {
        for (name, value) in buffered.headers() {
            if is_stripped_header(name, &connection_listed) {
                continue;
            }
            headers.append(name.clone(), value.clone());
        }
        if let Some(host) = host_override
            && let Ok(hv) = http::HeaderValue::from_str(host)
        {
            headers.insert(http::header::HOST, hv);
        }
    }

    let body = Full::new(buffered.body().clone())
        .map_err(|never: std::convert::Infallible| match never {})
        .boxed();
    builder
        .body(body)
        .map_err(|e| format!("build request: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_routed_keeps_inbound_host() -> Result<(), String> {
        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .header(http::header::HOST, "api.anthropic.com")
            .body(Bytes::from_static(b"{}"))
            .map_err(|e| e.to_string())?;
        let out = rebuild(&req, "http", "127.0.0.1:9988", None)?;
        assert_eq!(out.uri().scheme_str(), Some("http"));
        assert_eq!(
            out.uri().authority().map(|a| a.as_str()),
            Some("127.0.0.1:9988")
        );
        assert_eq!(header(&out, "host")?, "api.anthropic.com");
        Ok(())
    }

    #[test]
    fn rebuild_strips_hop_by_hop_and_framing_headers() -> Result<(), String> {
        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .header("content-length", "999")
            .header("transfer-encoding", "chunked")
            .header("connection", "keep-alive, x-custom-hop")
            .header("keep-alive", "timeout=5")
            .header("proxy-connection", "keep-alive")
            .header("upgrade", "h2c")
            .header("x-custom-hop", "drop-me")
            .header("x-api-key", "secret")
            .body(Bytes::from_static(b"{}"))
            .map_err(|e| e.to_string())?;

        let out = rebuild(&req, "http", "127.0.0.1:9988", None)?;

        for gone in [
            "content-length",
            "transfer-encoding",
            "connection",
            "keep-alive",
            "proxy-connection",
            "upgrade",
            "x-custom-hop",
        ] {
            assert!(
                out.headers().get(gone).is_none(),
                "header {gone:?} must be stripped from the rebuilt request"
            );
        }
        assert_eq!(header(&out, "x-api-key")?, "secret");
        Ok(())
    }

    #[test]
    fn rebuild_with_bad_authority_errors_replayably() -> Result<(), String> {
        let req = Request::builder()
            .method("POST")
            .uri("https://api.anthropic.com/v1/messages")
            .body(Bytes::from_static(b"{}"))
            .map_err(|e| e.to_string())?;
        let err = rebuild(&req, "http", "not a valid authority", None);
        assert!(
            err.is_err(),
            "a malformed authority must fail the build (→ Replayable → Direct)"
        );
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
