use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

const MAX_REQUEST_BYTES: usize = 128 * 1024;

pub(super) struct TestModel {
    address: std::net::SocketAddr,
    state: Arc<ModelState>,
    shutdown: CancellationToken,
    task: JoinHandle<io::Result<()>>,
}

struct ModelState {
    calls: AtomicUsize,
    entered: Notify,
    released: Semaphore,
    status: AtomicU16,
}

impl TestModel {
    pub(super) async fn start() -> anyhow::Result<Self> {
        let state = Arc::new(ModelState {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            released: Semaphore::new(0),
            status: AtomicU16::new(200),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let shutdown = CancellationToken::new();
        let task = tokio::spawn(serve(listener, Arc::clone(&state), shutdown.clone()));
        Ok(Self {
            address,
            state,
            shutdown,
            task,
        })
    }

    pub(super) fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(super) async fn entered(&self) -> anyhow::Result<()> {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.state.entered.notified(),
        )
        .await?;
        Ok(())
    }

    pub(super) fn calls(&self) -> usize {
        self.state.calls.load(Ordering::SeqCst)
    }

    pub(super) fn status(&self, status: u16) {
        self.state.status.store(status, Ordering::SeqCst);
    }

    pub(super) fn release(&self) {
        self.state.released.add_permits(1);
    }

    pub(super) async fn stop(self) -> anyhow::Result<()> {
        self.state.released.close();
        self.shutdown.cancel();
        self.task.await??;
        Ok(())
    }
}

async fn serve(
    listener: TcpListener,
    state: Arc<ModelState>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                connections.spawn(respond(stream, Arc::clone(&state), shutdown.clone()));
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => result.map_err(io::Error::other)??,
        }
    }
    while let Some(result) = connections.join_next().await {
        result.map_err(io::Error::other)??;
    }
    Ok(())
}

async fn respond(
    mut stream: TcpStream,
    state: Arc<ModelState>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    if !receive_request(&mut stream, &shutdown).await {
        return Ok(());
    }
    state.calls.fetch_add(1, Ordering::SeqCst);
    state.entered.notify_one();
    let Ok(_permit) = state.released.acquire().await else {
        return Ok(());
    };
    write_response(&mut stream, state.status.load(Ordering::SeqCst)).await;
    Ok(())
}

async fn receive_request(stream: &mut TcpStream, shutdown: &CancellationToken) -> bool {
    let read = tokio::select! {
        _ = shutdown.cancelled() => return false,
        result = read_request(stream) => result,
    };
    if let Err(error) = read {
        tracing::debug!(%error, "test model: request connection closed");
        return false;
    }
    true
}

async fn write_response(stream: &mut TcpStream, status: u16) {
    let body = if status == 200 {
        response_body()
    } else {
        String::new()
    };
    let response = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if let Err(error) = stream.write_all(response.as_bytes()).await {
        tracing::debug!(%error, "test model: response connection closed");
    }
}

async fn read_request(stream: &mut TcpStream) -> io::Result<()> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut target = None;
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        if request.len() + read > MAX_REQUEST_BYTES {
            return Err(io::Error::other(
                "test request exceeds bounded fixture size",
            ));
        }
        request.extend_from_slice(&chunk[..read]);
        if target.is_none() {
            target = request_length(&request)?;
        }
        if target.is_some_and(|size| request.len() >= size) {
            return Ok(());
        }
    }
}

fn request_length(request: &[u8]) -> io::Result<Option<usize>> {
    let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
        return Ok(None);
    };
    let headers = std::str::from_utf8(&request[..end]).map_err(io::Error::other)?;
    let length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .ok_or_else(|| io::Error::other("test request requires Content-Length"))?
        .1
        .trim()
        .parse::<usize>()
        .map_err(io::Error::other)?;
    let total = (end + 4)
        .checked_add(length)
        .filter(|size| *size <= MAX_REQUEST_BYTES)
        .ok_or_else(|| io::Error::other("test request exceeds bounded fixture size"))?;
    Ok(Some(total))
}

fn response_body() -> String {
    json!({
        "id": "test-generation",
        "choices": [{"message": {"content": json!({
            "reaction": "accepted", "delivery": "delivered", "confidence": 0.95,
            "rationale": "The requested retry was implemented.", "unmet_ask": "",
            "evidence_quality": "verified"
        }).to_string()}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 10, "cost": 0.001}
    })
    .to_string()
}
