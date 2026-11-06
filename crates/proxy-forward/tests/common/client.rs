use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use super::certs::install_crypto;

pub struct ClientResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

pub async fn connect_and_request(
    proxy_addr: SocketAddr,
    host: &str,
    trust_ca: &CertificateDer<'static>,
    method: &str,
    path: &str,
    body: &'static [u8],
) -> Result<ClientResponse> {
    connect_and_request_owned(proxy_addr, host, trust_ca, method, path, body.to_vec()).await
}

pub async fn connect_and_request_owned(
    proxy_addr: SocketAddr,
    host: &str,
    trust_ca: &CertificateDer<'static>,
    method: &str,
    path: &str,
    body: Vec<u8>,
) -> Result<ClientResponse> {
    install_crypto();

    let mut tcp = TcpStream::connect(proxy_addr)
        .await
        .context("connect proxy")?;
    let connect = format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n");
    tcp.write_all(connect.as_bytes())
        .await
        .context("send CONNECT")?;
    read_until_header_end(&mut tcp).await?;

    let mut roots = rustls::RootCertStore::empty();
    roots.add(trust_ca.clone()).context("trust ca")?;
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(host.to_string()).context("server name")?;
    let tls = connector
        .connect(server_name, tcp)
        .await
        .context("client TLS handshake")?;

    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("http1 handshake")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", format!("{host}:443"))
        .header("x-api-key", "test-key")
        .header("anthropic-version", "2023-06-01")
        .body(Full::new(Bytes::from(body)))
        .context("build req")?;

    let resp = sender.send_request(req).await.context("send request")?;
    Ok(collect_response(resp).await)
}

pub async fn connect_and_request_h2(
    proxy_addr: SocketAddr,
    host: &str,
    trust_ca: &CertificateDer<'static>,
    method: &str,
    path: &str,
    body: &'static [u8],
) -> Result<(ClientResponse, Vec<u8>)> {
    install_crypto();

    let mut tcp = TcpStream::connect(proxy_addr)
        .await
        .context("connect proxy")?;
    let connect = format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n");
    tcp.write_all(connect.as_bytes())
        .await
        .context("send CONNECT")?;
    read_until_header_end(&mut tcp).await?;

    let mut roots = rustls::RootCertStore::empty();
    roots.add(trust_ca.clone()).context("trust ca")?;
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"h2".to_vec()];
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(host.to_string()).context("server name")?;
    let tls = connector
        .connect(server_name, tcp)
        .await
        .context("client TLS handshake")?;

    let alpn = tls
        .get_ref()
        .1
        .alpn_protocol()
        .map(|p| p.to_vec())
        .unwrap_or_default();
    if alpn != b"h2" {
        return Err(anyhow!("server did not negotiate h2 ALPN (got {alpn:?})"));
    }

    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http2::handshake(TokioExecutor::new(), io)
        .await
        .context("http2 handshake")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", format!("{host}:443"))
        .header("x-api-key", "test-key")
        .header("anthropic-version", "2023-06-01")
        .body(Full::new(Bytes::from_static(body)))
        .context("build req")?;

    let resp = sender.send_request(req).await.context("send h2 request")?;
    Ok((collect_response(resp).await, alpn))
}

pub struct UpgradeHandshake {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub upgraded: TokioIo<hyper::upgrade::Upgraded>,
}

pub async fn connect_and_upgrade(
    proxy_addr: SocketAddr,
    host: &str,
    trust_ca: &CertificateDer<'static>,
    path: &str,
) -> Result<UpgradeHandshake> {
    install_crypto();

    let mut tcp = TcpStream::connect(proxy_addr)
        .await
        .context("connect proxy")?;
    let connect = format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n");
    tcp.write_all(connect.as_bytes())
        .await
        .context("send CONNECT")?;
    read_until_header_end(&mut tcp).await?;

    let mut roots = rustls::RootCertStore::empty();
    roots.add(trust_ca.clone()).context("trust ca")?;
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(host.to_string()).context("server name")?;
    let tls = connector
        .connect(server_name, tcp)
        .await
        .context("client TLS handshake")?;

    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("http1 handshake")?;
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header("host", format!("{host}:443"))
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .body(Full::new(Bytes::new()))
        .context("build upgrade request")?;

    let mut resp = sender
        .send_request(req)
        .await
        .context("send upgrade request")?;
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).to_string(),
            )
        })
        .collect();

    let upgraded = hyper::upgrade::on(&mut resp)
        .await
        .context("client-side upgrade did not complete")?;

    Ok(UpgradeHandshake {
        status,
        headers,
        upgraded: TokioIo::new(upgraded),
    })
}

pub async fn connect_and_stream(
    proxy_addr: SocketAddr,
    host: &str,
    trust_ca: &CertificateDer<'static>,
    path: &str,
) -> Result<Response<Incoming>> {
    install_crypto();

    let mut tcp = TcpStream::connect(proxy_addr)
        .await
        .context("connect proxy")?;
    let connect = format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n");
    tcp.write_all(connect.as_bytes())
        .await
        .context("send CONNECT")?;
    read_until_header_end(&mut tcp).await?;

    let mut roots = rustls::RootCertStore::empty();
    roots.add(trust_ca.clone()).context("trust ca")?;
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(host.to_string()).context("server name")?;
    let tls = connector
        .connect(server_name, tcp)
        .await
        .context("client TLS handshake")?;

    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("http1 handshake")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header("host", format!("{host}:443"))
        .header("accept", "text/event-stream")
        .header("authorization", "Bearer sk-ant-oat-test")
        .header("anthropic-version", "2023-06-01")
        .header("user-agent", "claude-cli/2.1.258 (external, cli)")
        .body(Full::new(Bytes::new()))
        .context("build req")?;

    sender.send_request(req).await.context("send request")
}

async fn collect_response<B>(resp: Response<B>) -> ClientResponse
where
    B: hyper::body::Body,
    B::Error: std::fmt::Debug,
{
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).to_string(),
            )
        })
        .collect();
    let body = resp
        .into_body()
        .collect()
        .await
        .map(|b| b.to_bytes().to_vec())
        .unwrap_or_default();
    ClientResponse {
        status,
        body,
        headers,
    }
}

async fn read_until_header_end(stream: &mut TcpStream) -> Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let n = stream.read(&mut chunk).await.context("read ack")?;
        if n == 0 {
            return Err(anyhow!("proxy closed before sending CONNECT ack"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(());
        }
    }
}
