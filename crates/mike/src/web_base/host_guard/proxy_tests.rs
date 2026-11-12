use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::app::proxy_router;
use crate::config::ProxyConfig;

async fn serve_proxy() -> anyhow::Result<std::net::SocketAddr> {
    let state = crate::config::AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, proxy_router(state)).await;
    });
    Ok(addr)
}

async fn post(
    addr: std::net::SocketAddr,
    host: &str,
    origin: Option<&str>,
) -> anyhow::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    let origin = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    let body = "{}";
    let request = format!(
        "POST /v1/messages HTTP/1.1\r\nHost: {host}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

#[tokio::test]
async fn proxy_port_hostile_origin_over_tcp_is_refused_before_capture() -> anyhow::Result<()> {
    let addr = serve_proxy().await?;
    let response = post(addr, &addr.to_string(), Some("http://evil.example")).await?;
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert!(response.contains("origin not allowed"), "{response}");
    Ok(())
}

#[tokio::test]
async fn proxy_port_forged_host_over_tcp_is_refused_before_capture() -> anyhow::Result<()> {
    let addr = serve_proxy().await?;
    let response = post(addr, "rebind.evil.example", None).await?;
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert!(response.contains("host not allowed"), "{response}");
    Ok(())
}

#[tokio::test]
async fn proxy_port_originless_client_over_tcp_reaches_the_handler() -> anyhow::Result<()> {
    let addr = serve_proxy().await?;
    let response = post(addr, &addr.to_string(), None).await?;
    assert!(!response.starts_with("HTTP/1.1 403"), "{response}");
    let response = post(addr, "api.anthropic.com", None).await?;
    assert!(!response.starts_with("HTTP/1.1 403"), "{response}");
    Ok(())
}
