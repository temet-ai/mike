use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use http_body_util::{BodyExt, Empty};
use hyper::Request;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tokio_util::sync::CancellationToken;

pub const DOWN_AFTER_CONSECUTIVE_FAILURES: u32 = 2;
pub const HANDLER_WORST_CASE: Duration = Duration::from_secs(4);
pub const MIN_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HealthState {
    Unknown = 0,
    Up = 1,
    Down = 2,
}

impl HealthState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => HealthState::Up,
            2 => HealthState::Down,
            _ => HealthState::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyHealth {
    state: Arc<AtomicU8>,
    bypassed: Arc<AtomicU64>,
}

impl ProxyHealth {
    pub fn unknown() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(HealthState::Unknown as u8)),
            bypassed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn note_bypass(&self) -> u64 {
        self.bypassed.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn take_bypassed(&self) -> u64 {
        self.bypassed.swap(0, Ordering::AcqRel)
    }

    pub fn get(&self) -> HealthState {
        HealthState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn set(&self, s: HealthState) {
        self.state.store(s as u8, Ordering::Release);
    }

    pub fn is_up(&self) -> bool {
        self.get() == HealthState::Up
    }
}

impl Default for ProxyHealth {
    fn default() -> Self {
        Self::unknown()
    }
}

pub async fn run_prober(
    health: ProxyHealth,
    health_url: String,
    routed_authority: String,
    interval: Duration,
    timeout: Duration,
    cancel: CancellationToken,
) {
    let client: Client<HttpConnector, Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let timeout = effective_probe_timeout(timeout);
    let mut failures = 0u32;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!("health prober cancelled");
                return;
            }
            _ = tokio::time::sleep(interval) => {}
        }

        let up = probe_once(&client, &health_url, &routed_authority, timeout).await;
        record_probe_result(&health, &mut failures, up, &health_url);
    }
}

pub fn effective_probe_timeout(configured: Duration) -> Duration {
    configured.max(MIN_PROBE_TIMEOUT)
}

pub fn record_probe_result(health: &ProxyHealth, failures: &mut u32, up: bool, health_url: &str) {
    let new_state = if up {
        *failures = 0;
        HealthState::Up
    } else {
        *failures = failures.saturating_add(1);
        if *failures < DOWN_AFTER_CONSECUTIVE_FAILURES {
            return;
        }
        HealthState::Down
    };
    let prev = health.get();
    health.set(new_state);
    if prev != new_state {
        let bypassed_while_not_up = if new_state == HealthState::Up {
            health.take_bypassed()
        } else {
            0
        };
        tracing::info!(
            ?prev,
            ?new_state,
            %health_url,
            bypassed_while_not_up,
            "routed-leg health changed"
        );
    }
}

async fn probe_once(
    client: &Client<HttpConnector, Empty<Bytes>>,
    health_url: &str,
    routed_authority: &str,
    timeout: Duration,
) -> bool {
    if !routed_leg_connects(routed_authority, timeout).await {
        return false;
    }

    let Some(req) = build_health_request(health_url) else {
        return false;
    };

    let Some(resp) = send_health_request(client, req, timeout).await else {
        return false;
    };

    if resp.status() != hyper::StatusCode::OK {
        tracing::debug!(status = %resp.status(), "health probe non-200");
        return false;
    }

    let Some(body) = read_health_body(resp).await else {
        return false;
    };

    body_status_is_forwarding(&body)
}

fn build_health_request(health_url: &str) -> Option<Request<Empty<Bytes>>> {
    let uri = parse_health_uri(health_url)?;
    build_health_get(uri, health_url)
}

fn parse_health_uri(health_url: &str) -> Option<hyper::Uri> {
    match health_url.parse() {
        Ok(u) => Some(u),
        Err(e) => {
            tracing::warn!(error = %e, %health_url, "health URL is not a valid URI");
            None
        }
    }
}

fn build_health_get(uri: hyper::Uri, health_url: &str) -> Option<Request<Empty<Bytes>>> {
    match Request::builder()
        .method("GET")
        .uri(uri)
        .header(hyper::header::HOST, host_header_for(health_url))
        .body(Empty::<Bytes>::new())
    {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(error = %e, "failed to build health request");
            None
        }
    }
}

async fn send_health_request(
    client: &Client<HttpConnector, Empty<Bytes>>,
    req: Request<Empty<Bytes>>,
    timeout: Duration,
) -> Option<hyper::Response<hyper::body::Incoming>> {
    match tokio::time::timeout(timeout, client.request(req)).await {
        Ok(Ok(resp)) => Some(resp),
        Ok(Err(e)) => {
            log_health_probe_transport_error(&e);
            None
        }
        Err(_) => {
            log_health_probe_timed_out();
            None
        }
    }
}

fn log_health_probe_transport_error(e: &hyper_util::client::legacy::Error) {
    tracing::debug!(error = %e, "health probe transport error");
}

fn log_health_probe_timed_out() {
    tracing::debug!("health probe timed out");
}

async fn read_health_body(resp: hyper::Response<hyper::body::Incoming>) -> Option<Bytes> {
    match resp.into_body().collect().await {
        Ok(b) => Some(b.to_bytes()),
        Err(e) => {
            tracing::debug!(error = %e, "failed to read health body");
            None
        }
    }
}

async fn routed_leg_connects(authority: &str, timeout: Duration) -> bool {
    match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(authority)).await {
        Ok(Ok(_stream)) => true,
        Ok(Err(e)) => {
            log_routed_leg_connect_failed(authority, &e);
            false
        }
        Err(_) => {
            log_routed_leg_connect_timed_out(authority);
            false
        }
    }
}

fn log_routed_leg_connect_failed(authority: &str, e: &std::io::Error) {
    tracing::debug!(error = %e, authority, "routed-leg TCP connect failed");
}

fn log_routed_leg_connect_timed_out(authority: &str) {
    tracing::debug!(authority, "routed-leg TCP connect timed out");
}

fn host_header_for(url: &str) -> String {
    url.parse::<hyper::Uri>()
        .ok()
        .and_then(|u| u.authority().map(|a| a.to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn body_status_is_forwarding(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("status")
                .and_then(|s| s.as_str())
                .map(|s| s == "ok" || s == "degraded")
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
