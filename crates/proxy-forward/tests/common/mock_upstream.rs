use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

use super::certs::{install_crypto, mint_cert};

pub struct StubHandle {
    pub addr: SocketAddr,
    pub received: Arc<tokio::sync::Mutex<Vec<RecordedRequest>>>,
}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    pub fn has_header(&self, name: &str, value: &str) -> bool {
        self.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case(name) && v == value)
    }
}

pub async fn plain_http_stub(reply_body: &'static str) -> Result<StubHandle> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind plain stub")?;
    let addr = listener.local_addr().context("plain stub addr")?;
    let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let received = Arc::clone(&received_clone);
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let service = service_fn(move |req| {
                    let received = Arc::clone(&received);
                    async move {
                        let rec = record(req).await;
                        received.lock().await.push(rec);
                        Ok::<_, Infallible>(text_response(reply_body))
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    Ok(StubHandle { addr, received })
}

pub async fn tls_http_stub(host: &str, reply_body: &'static str) -> Result<(StubHandle, String)> {
    install_crypto();
    let cert = mint_cert(host)?;
    let ca_pem = cert.ca_pem.clone();

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert.leaf_chain.clone(), cert.leaf_key.clone_key())
        .context("tls stub server config")?;
    let mut server_config = server_config;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind tls stub")?;
    let addr = listener.local_addr().context("tls stub addr")?;
    let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            let received = Arc::clone(&received_clone);
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(stream).await else {
                    return;
                };
                let io = TokioIo::new(tls);
                let service = service_fn(move |req| {
                    let received = Arc::clone(&received);
                    async move {
                        let rec = record(req).await;
                        received.lock().await.push(rec);
                        Ok::<_, Infallible>(text_response(reply_body))
                    }
                });
                let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    Ok((StubHandle { addr, received }, ca_pem))
}

pub async fn tls_upgrade_stub(host: &str) -> Result<(SocketAddr, String)> {
    install_crypto();
    let cert = mint_cert(host)?;
    let ca_pem = cert.ca_pem.clone();

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert.leaf_chain.clone(), cert.leaf_key.clone_key())
        .context("tls upgrade stub server config")?;
    let mut server_config = server_config;
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind tls upgrade stub")?;
    let addr = listener.local_addr().context("tls upgrade stub addr")?;

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(stream).await else {
                    return;
                };
                let io = TokioIo::new(tls);
                let service = service_fn(handle_upgrade_request);
                let conn = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .with_upgrades();
                let _ = conn.await;
            });
        }
    });

    Ok((addr, ca_pem))
}

async fn handle_upgrade_request(
    mut req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if req.headers().get(http::header::UPGRADE).is_none() {
        return Ok(Response::builder()
            .status(400)
            .body(Full::new(Bytes::from_static(b"not an upgrade request")))
            .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))));
    }

    let on_upgrade = hyper::upgrade::on(&mut req);
    tokio::spawn(async move {
        let Ok(upgraded) = on_upgrade.await else {
            return;
        };
        let mut io = TokioIo::new(upgraded);
        let mut buf = [0u8; 4096];
        loop {
            match io.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if io.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(Response::builder()
        .status(101)
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))))
}

pub async fn tcp_echo_stub() -> Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind echo")?;
    let addr = listener.local_addr().context("echo addr")?;
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    Ok(addr)
}

pub async fn accept_then_reset_stub() -> Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind reset stub")?;
    let addr = listener.local_addr().context("reset stub addr")?;
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            drop(stream);
        }
    });
    Ok(addr)
}

pub async fn stoppable_accept_stub() -> Result<(SocketAddr, CancellationToken)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind stoppable stub")?;
    let addr = listener.local_addr().context("stoppable stub addr")?;
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    drop(stream);
                }
            }
        }
    });
    Ok((addr, cancel))
}

pub async fn wait_until_unbound(addr: SocketAddr) -> Result<()> {
    for _ in 0..400 {
        if tokio::net::TcpStream::connect(addr).await.is_err() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(anyhow!("port {addr} never stopped accepting connections"))
}

async fn record(req: Request<Incoming>) -> RecordedRequest {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).to_string(),
            )
        })
        .collect();
    let body = req
        .into_body()
        .collect()
        .await
        .map(|b| b.to_bytes().to_vec())
        .unwrap_or_default();
    RecordedRequest {
        method,
        path,
        headers,
        body,
    }
}

fn text_response(body: &'static str) -> Response<Full<Bytes>> {
    let builder = Response::builder()
        .status(200)
        .header("content-type", "text/plain");
    match builder.body(Full::new(Bytes::from_static(body.as_bytes()))) {
        Ok(resp) => resp,
        Err(_) => Response::new(Full::new(Bytes::new())),
    }
}

pub struct ChannelBody(tokio::sync::mpsc::Receiver<Bytes>);

impl hyper::body::Body for ChannelBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.get_mut().0.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => Poll::Ready(Some(Ok(Frame::data(chunk)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn sse_frame(seq: usize) -> String {
    format!("id: {seq}\nevent: client_event\ndata: {{\"seq\":{seq}}}\n\n")
}

pub async fn plain_sse_stub(events: usize, gap: Duration) -> Result<StubHandle> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind sse stub")?;
    let addr = listener.local_addr().context("sse stub addr")?;
    let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let received = Arc::clone(&received_clone);
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let service = service_fn(move |req| {
                    let received = Arc::clone(&received);
                    async move {
                        let rec = record(req).await;
                        received.lock().await.push(rec);
                        let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
                        tokio::spawn(async move {
                            for seq in 1..=events {
                                tokio::time::sleep(gap).await;
                                if tx.send(Bytes::from(sse_frame(seq))).await.is_err() {
                                    break;
                                }
                            }
                        });
                        let resp = Response::builder()
                            .status(200)
                            .header("content-type", "text/event-stream")
                            .header("cache-control", "no-cache")
                            .body(ChannelBody(rx))
                            .unwrap_or_else(|_| {
                                let (_tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
                                Response::new(ChannelBody(rx))
                            });
                        Ok::<_, Infallible>(resp)
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    Ok(StubHandle { addr, received })
}
