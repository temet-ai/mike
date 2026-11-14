use std::path::Path;

use axum::Router;
use axum::extract::Request;
use axum::http::Method;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream, UnixStream};

use super::context::upstream;
use super::relay::{RUN_HEADER, RelayServer, RelayTargets, Route, route};
use super::relay_inside::{bind_all, pump};

#[test]
fn relay_route_allows_only_messages_and_read_only_inspect() {
    let cases = [
        (Method::POST, "/v1/messages", Route::Proxy),
        (Method::POST, "/v1/messages/count_tokens", Route::Proxy),
        (Method::GET, "/api/sessions", Route::Inspect),
        (Method::GET, "/api/requests/12/body", Route::Inspect),
    ];
    for (method, path, expected) in cases {
        assert_eq!(route(&method, path), expected, "{method} {path}");
    }
    let refused = [
        (Method::POST, "/api/sessions"),
        (Method::PATCH, "/api/wright/engineer-limits"),
        (Method::DELETE, "/api/lessons/3"),
        (Method::PUT, "/api/settings"),
        (Method::GET, "/api/wright/problems"),
        (Method::GET, "/api/wright"),
        (Method::GET, "/API/WRIGHT/findings"),
        (Method::GET, "/api/settings/assistant/anthropic/credentials"),
        (Method::GET, "/api/config/snapshot"),
        (Method::GET, "/api/ws"),
        (Method::GET, "/api/%77right/problems"),
        (Method::GET, "/api/sessions/../wright/problems"),
        (Method::GET, "//api/wright"),
        (Method::GET, "/v1/messages"),
        (Method::POST, "/v1/messages/batches"),
        (Method::POST, "/v1/complete"),
        (Method::GET, "/metrics"),
        (Method::GET, "/"),
    ];
    for (method, path) in refused {
        assert!(
            matches!(route(&method, path), Route::Refuse(_)),
            "{method} {path} must be refused"
        );
    }
}

async fn echo(request: Request) -> axum::Json<Value> {
    let headers: serde_json::Map<String, Value> = request
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_owned(), json!(v.to_str().unwrap_or_default())))
        .collect();
    axum::Json(json!({
        "method": request.method().as_str(),
        "path": request.uri().path(),
        "headers": headers,
    }))
}

async fn upstream_server() -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, Router::new().fallback(echo)).await;
    });
    Ok(addr.to_string())
}

async fn exchange<S>(
    mut stream: S,
    method: &str,
    path: &str,
    extra: &str,
) -> anyhow::Result<(u16, Value)>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let request = format!(
        "{method} {path} HTTP/1.1\r\nhost: relay\r\n{extra}content-length: 2\r\nconnection: close\r\n\r\n{{}}"
    );
    stream.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("no status line in {text}"))?
        .parse()?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("no body in {text}"))?;
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        body.lines().nth(1).unwrap_or_default().to_owned()
    } else {
        body.to_owned()
    };
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

async fn over_socket(
    socket: &Path,
    method: &str,
    path: &str,
    extra: &str,
) -> anyhow::Result<(u16, Value)> {
    exchange(UnixStream::connect(socket).await?, method, path, extra).await
}

async fn relay(socket: &Path) -> anyhow::Result<RelayServer> {
    let proxy = upstream_server().await?;
    let inspect = upstream_server().await?;
    let ups = upstream(&proxy, &inspect).map_err(anyhow::Error::msg)?;
    let targets = RelayTargets {
        run_id: 41,
        proxy: ups.proxy,
        inspect: ups.inspect,
        client: ups.client,
    };
    RelayServer::start(socket, targets).map_err(anyhow::Error::msg)
}

#[tokio::test]
async fn relay_refuses_writes_and_wright_routes_with_403() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let socket = tmp.path().join("relay.sock");
    let server = relay(&socket).await?;
    for (method, path) in [
        ("POST", "/api/lessons"),
        ("PATCH", "/api/wright/engineer-limits"),
        ("GET", "/api/wright/problems"),
        ("GET", "/api/settings"),
        ("DELETE", "/api/sessions/1"),
    ] {
        let (status, body) = over_socket(&socket, method, path, "").await?;
        assert_eq!(status, 403, "{method} {path}");
        assert_eq!(body["path"], path);
        assert!(body["reason"].as_str().is_some_and(|r| !r.is_empty()));
    }
    server.stop();
    Ok(())
}

#[tokio::test]
async fn relay_passes_reads_and_strips_credentials_from_inspect() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let socket = tmp.path().join("relay.sock");
    let server = relay(&socket).await?;
    let extra = format!("authorization: Bearer stolen\r\ncookie: c=1\r\n{RUN_HEADER}: 999\r\n");
    let (status, seen) = over_socket(&socket, "GET", "/api/sessions?limit=2", &extra).await?;
    assert_eq!(status, 200);
    assert_eq!(seen["path"], "/api/sessions");
    assert!(seen["headers"].get("authorization").is_none());
    assert!(seen["headers"].get("cookie").is_none());
    assert_eq!(seen["headers"][RUN_HEADER], "41");
    let (status, seen) = over_socket(
        &socket,
        "POST",
        "/v1/messages",
        "authorization: Bearer oauth\r\n",
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(seen["method"], "POST");
    assert_eq!(seen["headers"]["authorization"], "Bearer oauth");
    assert_eq!(seen["headers"][RUN_HEADER], "41");
    server.stop();
    Ok(())
}

#[tokio::test]
async fn inside_relay_carries_tcp_to_the_socket() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let socket = tmp.path().join("relay.sock");
    let server = relay(&socket).await?;
    let listeners = bind_all(&["127.0.0.1:0".parse()?])
        .await
        .map_err(anyhow::Error::msg)?;
    let mut addrs = Vec::new();
    for listener in listeners {
        addrs.push(listener.local_addr()?);
        tokio::spawn(pump(listener, socket.clone()));
    }
    let tcp = TcpStream::connect(addrs[0]).await?;
    let (status, seen) = exchange(tcp, "GET", "/api/health", "").await?;
    assert_eq!(status, 200);
    assert_eq!(seen["path"], "/api/health");
    let tcp = TcpStream::connect(addrs[0]).await?;
    let (status, _) = exchange(tcp, "POST", "/api/wright/messages", "").await?;
    assert_eq!(status, 403);
    server.stop();
    Ok(())
}
