use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;

use crate::error::ForwardError;
use crate::mitm::{self, MitmContext};
use crate::tunnel;

const MAX_CONNECT_HEADER_BYTES: usize = 8 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct ConnectTarget {
    pub host: String,
    pub port: u16,
}

pub async fn handle(mut client: TcpStream, ctx: MitmContext) -> Result<(), ForwardError> {
    let (target, leftover) = match read_connect_target(&mut client).await {
        Ok(t) => t,
        Err(e) => {
            send_bad_request(&mut client).await;
            return Err(e);
        }
    };

    ack_connect(&mut client).await?;

    let leftover_bytes = leftover.len();
    let client = PrefixedStream::new(leftover, client);

    dispatch_to_leg(client, &target, leftover_bytes, ctx).await
}

async fn send_bad_request(client: &mut TcpStream) {
    let _ = client
        .write_all(b"HTTP/1.1 400 Bad Request\r\nconnection: close\r\n\r\n")
        .await;
}

async fn ack_connect(client: &mut TcpStream) -> Result<(), ForwardError> {
    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .map_err(ForwardError::Io)
}

async fn dispatch_to_leg(
    client: PrefixedStream<TcpStream>,
    target: &ConnectTarget,
    leftover_bytes: usize,
    ctx: MitmContext,
) -> Result<(), ForwardError> {
    if is_intercept_host(&target.host, &ctx.intercept_host) {
        serve_mitm_leg(client, target, leftover_bytes, ctx).await
    } else {
        serve_tunnel_leg(client, target, leftover_bytes).await
    }
}

async fn serve_mitm_leg(
    client: PrefixedStream<TcpStream>,
    target: &ConnectTarget,
    leftover_bytes: usize,
    ctx: MitmContext,
) -> Result<(), ForwardError> {
    tracing::info!(host = %target.host, port = target.port, leftover_bytes, "CONNECT → MITM (captured leg)");
    mitm::serve(client, ctx).await
}

async fn serve_tunnel_leg(
    client: PrefixedStream<TcpStream>,
    target: &ConnectTarget,
    leftover_bytes: usize,
) -> Result<(), ForwardError> {
    if is_uncaptured_model_host(&target.host) {
        warn_uncaptured_model_tunnel(target);
    } else {
        log_blind_tunnel(target, leftover_bytes);
    }
    tunnel::blind(client, &target.host, target.port).await
}

fn warn_uncaptured_model_tunnel(target: &ConnectTarget) {
    tracing::warn!(host = %target.host, port = target.port, "CONNECT → blind tunnel to an OpenAI model host: Codex traffic through this sidecar is NOT captured; start Codex with `cdx` so it reaches mike's /codex route");
}

fn log_blind_tunnel(target: &ConnectTarget, leftover_bytes: usize) {
    tracing::info!(host = %target.host, port = target.port, leftover_bytes, "CONNECT → blind tunnel (NOT captured)");
}

const UNCAPTURED_MODEL_HOSTS: &[&str] = &["api.openai.com", "chatgpt.com"];

pub(crate) fn is_uncaptured_model_host(host: &str) -> bool {
    UNCAPTURED_MODEL_HOSTS
        .iter()
        .any(|known| host.eq_ignore_ascii_case(known))
}

fn is_intercept_host(host: &str, intercept_host: &str) -> bool {
    host.eq_ignore_ascii_case(intercept_host)
}

async fn read_connect_target<S>(client: &mut S) -> Result<(ConnectTarget, Vec<u8>), ForwardError>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let n = client.read(&mut chunk).await.map_err(ForwardError::Io)?;
        if n == 0 {
            return Err(ForwardError::BadConnectTarget(
                "client closed before sending a full CONNECT request".to_string(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
        if buf.len() > MAX_CONNECT_HEADER_BYTES {
            return Err(ForwardError::BadConnectTarget(
                "CONNECT headers exceeded size limit".to_string(),
            ));
        }
    };

    let target = parse_connect_line(&buf)?;
    let leftover = buf[header_end..].to_vec();
    Ok((target, leftover))
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

pub fn parse_connect_line(buf: &[u8]) -> Result<ConnectTarget, ForwardError> {
    let text = std::str::from_utf8(buf)
        .map_err(|_| ForwardError::BadConnectTarget("request was not UTF-8".to_string()))?;
    let first_line = text
        .lines()
        .next()
        .ok_or_else(|| ForwardError::BadConnectTarget("empty request".to_string()))?;

    let mut parts = first_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| ForwardError::BadConnectTarget("missing method".to_string()))?;
    if !method.eq_ignore_ascii_case("CONNECT") {
        return Err(ForwardError::BadConnectTarget(format!(
            "expected CONNECT, got {method:?}"
        )));
    }
    let authority = parts
        .next()
        .ok_or_else(|| ForwardError::BadConnectTarget("missing CONNECT target".to_string()))?;

    split_host_port(authority)
}

fn split_host_port(authority: &str) -> Result<ConnectTarget, ForwardError> {
    let (host, port) = authority.rsplit_once(':').ok_or_else(|| {
        ForwardError::BadConnectTarget(format!("CONNECT target {authority:?} has no port"))
    })?;
    if host.is_empty() {
        return Err(ForwardError::BadConnectTarget(format!(
            "CONNECT target {authority:?} has empty host"
        )));
    }
    let port: u16 = port.parse().map_err(|_| {
        ForwardError::BadConnectTarget(format!("CONNECT target {authority:?} has bad port"))
    })?;
    Ok(ConnectTarget {
        host: host.to_string(),
        port,
    })
}

pub(crate) struct PrefixedStream<S> {
    prefix: Vec<u8>,
    pos: usize,
    inner: S,
}

impl<S> PrefixedStream<S> {
    fn new(prefix: Vec<u8>, inner: S) -> Self {
        Self {
            prefix,
            pos: 0,
            inner,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefixedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.pos < self.prefix.len() && buf.remaining() > 0 {
            let remaining = &self.prefix[self.pos..];
            let n = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..n]);
            self.pos += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefixedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_connect_line() -> Result<(), ForwardError> {
        let raw = b"CONNECT api.anthropic.com:443 HTTP/1.1\r\nHost: api.anthropic.com:443\r\n\r\n";
        let target = parse_connect_line(raw)?;
        assert_eq!(target.host, "api.anthropic.com");
        assert_eq!(target.port, 443);
        Ok(())
    }

    #[tokio::test]
    async fn read_connect_target_preserves_pipelined_bytes() -> Result<(), ForwardError> {
        let raw: &[u8] =
            b"CONNECT api.anthropic.com:443 HTTP/1.1\r\nHost: h\r\n\r\n\x16\x03\x01HELLO";
        let mut client = raw;
        let (target, leftover) = read_connect_target(&mut client).await?;
        assert_eq!(target.host, "api.anthropic.com");
        assert_eq!(leftover, b"\x16\x03\x01HELLO");
        Ok(())
    }

    #[tokio::test]
    async fn read_connect_target_no_pipeline_yields_empty_leftover() -> Result<(), ForwardError> {
        let raw: &[u8] = b"CONNECT api.anthropic.com:443 HTTP/1.1\r\nHost: h\r\n\r\n";
        let mut client = raw;
        let (_target, leftover) = read_connect_target(&mut client).await?;
        assert!(leftover.is_empty(), "no pipelined bytes → empty leftover");
        Ok(())
    }

    #[tokio::test]
    async fn prefixed_stream_yields_prefix_then_inner() -> std::io::Result<()> {
        let inner: &[u8] = b"-INNER";
        let mut stream = PrefixedStream::new(b"PREFIX".to_vec(), inner);
        let mut out = Vec::new();
        stream.read_to_end(&mut out).await?;
        assert_eq!(out, b"PREFIX-INNER");
        Ok(())
    }

    fn assert_bad_target(raw: &[u8]) {
        match parse_connect_line(raw) {
            Err(ForwardError::BadConnectTarget(_)) => {}
            other => panic!("expected BadConnectTarget, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_connect_method() {
        assert_bad_target(b"GET http://example.com/ HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn rejects_missing_port() {
        assert_bad_target(b"CONNECT api.anthropic.com HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn rejects_empty_host() {
        assert_bad_target(b"CONNECT :443 HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn rejects_bad_port() {
        assert_bad_target(b"CONNECT host:notaport HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn handles_host_with_explicit_port() -> Result<(), ForwardError> {
        let raw = b"CONNECT some.host.example:8443 HTTP/1.1\r\n\r\n";
        let target = parse_connect_line(raw)?;
        assert_eq!(target.host, "some.host.example");
        assert_eq!(target.port, 8443);
        Ok(())
    }

    #[test]
    fn mixed_case_connect_host_routes_to_mitm() -> Result<(), ForwardError> {
        let target = parse_connect_line(b"CONNECT API.Anthropic.Com:443 HTTP/1.1\r\n\r\n")?;
        assert!(
            is_intercept_host(&target.host, "api.anthropic.com"),
            "mixed-case CONNECT host must route to MITM, not tunnel"
        );
        assert!(
            !is_intercept_host("example.com", "api.anthropic.com"),
            "a non-intercept host must NOT route to MITM"
        );
        Ok(())
    }

    #[test]
    fn openai_model_hosts_are_flagged_as_uncaptured_tunnels() {
        assert!(is_uncaptured_model_host("api.openai.com"));
        assert!(is_uncaptured_model_host("ChatGPT.com"));
        assert!(!is_uncaptured_model_host("api.anthropic.com"));
        assert!(!is_uncaptured_model_host("example.com"));
    }

    #[test]
    fn find_header_end_locates_terminator() {
        assert_eq!(find_header_end(b"CONNECT x:1 HTTP/1.1\r\n\r\n"), Some(24));
        assert_eq!(find_header_end(b"partial\r\n"), None);
    }
}
