use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::ledger::Ledger;
use super::proxy_ledger::ProxyLedger;
use super::runner::Runner;
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::{TerminalBackend, TrustGate};

pub const MIKE_USER_SERVER: &str = r#"{"mcpServers":{"mike":{"command":"mike-mcp"}}}"#;

pub fn toks(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn runner<'a>(
    backend: &'a dyn TerminalBackend,
    config: &'a LaunchConfig,
    home: &Path,
) -> Runner<'a> {
    let claude_json = home.join(".claude.json");
    if !claude_json.exists()
        && let Err(error) = std::fs::write(&claude_json, MIKE_USER_SERVER)
    {
        tracing::warn!(%error, "test kit could not write the user server file");
    }
    Runner {
        backend,
        ledger: Ledger::none(),
        config,
        proxy: None,
        trust: TrustGate::new(claude_json),
        briefs_dir: home.join("briefs"),
        inspect_addr: None,
        mcp_server: None,
        backend_reason: None,
    }
}

pub fn project_dir(home: &Path) -> PathBuf {
    let dir = home.join("proj");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%error, "test kit could not create the project folder");
    }
    std::fs::canonicalize(&dir).unwrap_or(dir)
}

pub fn fake() -> FakeBackend {
    FakeBackend::available()
}

pub fn open_tokens(folder: &std::path::Path, rest: &[&str]) -> Vec<String> {
    let mut tokens = vec![folder.to_string_lossy().into_owned()];
    tokens.extend(toks(rest));
    tokens
}

pub const HELLO: &str =
    r#"{"database":true,"launch_brief":false,"settings":[["launch_max_surfaces",9]]}"#;

#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub target: String,
    pub body: String,
}

pub struct FakeProxy {
    pub addr: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeProxy {
    pub async fn seen(&self) -> Vec<Seen> {
        self.seen.lock().await.clone()
    }

    pub async fn hit(&self, method: &str, target: &str) -> Option<Seen> {
        self.seen()
            .await
            .into_iter()
            .find(|s| s.method == method && s.target.starts_with(target))
    }

    pub async fn must_hit(&self, method: &str, target: &str) -> anyhow::Result<Seen> {
        let seen = self.seen().await;
        seen.iter()
            .find(|s| s.method == method && s.target.starts_with(target))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no {method} {target} among {seen:?}"))
    }
}

impl Drop for FakeProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub type Route = (&'static str, String, u16, String);

async fn read_request(stream: &mut TcpStream) -> Option<Seen> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let mut first = head.lines().next()?.split_whitespace();
    Some(Seen {
        method: first.next()?.to_string(),
        target: first.next()?.to_string(),
        body: String::from_utf8_lossy(&buf[head_end..]).to_string(),
    })
}

fn matches_route(prefix: &str, target: &str) -> bool {
    target
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('?') || prefix.ends_with('/'))
}

fn answer(routes: &[Route], seen: &Seen) -> (u16, String) {
    routes
        .iter()
        .find(|(m, prefix, _, _)| *m == seen.method && matches_route(prefix, &seen.target))
        .map_or((200, "{}".to_string()), |(_, _, status, body)| {
            (*status, body.clone())
        })
}

pub async fn fake_proxy(routes: Vec<Route>) -> anyhow::Result<FakeProxy> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let Some(request) = read_request(&mut stream).await else {
                continue;
            };
            let (status, body) = answer(&routes, &request);
            log.lock().await.push(request);
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    Ok(FakeProxy { addr, seen, task })
}

pub fn hello_route() -> Route {
    (
        "GET",
        "/api/launch-ledger".to_string(),
        200,
        HELLO.to_string(),
    )
}

pub async fn proxy_ledger(addr: &str) -> anyhow::Result<Ledger> {
    let (proxy, _) = ProxyLedger::connect(addr)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(Ledger::through_proxy(proxy))
}

pub fn quiet_config() -> LaunchConfig {
    LaunchConfig {
        channel_check_enabled: false,
        ..LaunchConfig::trusting()
    }
}
