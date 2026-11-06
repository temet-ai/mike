use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::ca::Ca;
use crate::cert_resolver::LeafResolver;
use crate::config::Config;
use crate::error::ForwardError;
use crate::health::{ProxyHealth, run_prober};
use crate::mitm::MitmContext;
use crate::upstream::UpstreamClients;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

fn load_extra_direct_roots(
    path: Option<&std::path::Path>,
) -> Result<Vec<rustls_pki_types::CertificateDer<'static>>, ForwardError> {
    use rustls_pki_types::CertificateDer;
    use rustls_pki_types::pem::PemObject;

    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let iter = CertificateDer::pem_file_iter(path)
        .map_err(|e| ForwardError::TlsConnect(format!("read extra CA {}: {e}", path.display())))?;
    let mut out = Vec::new();
    for cert in iter {
        let cert = cert.map_err(|e| {
            ForwardError::TlsConnect(format!("parse extra CA {}: {e}", path.display()))
        })?;
        out.push(cert);
    }
    if out.is_empty() {
        return Err(ForwardError::TlsConnect(format!(
            "extra CA {} held no certificates",
            path.display()
        )));
    }
    Ok(out)
}

fn build_acceptor(ca: &Ca, intercept_host: String) -> Result<TlsAcceptor, ForwardError> {
    let resolver = Arc::new(LeafResolver::new(ca, intercept_host));
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub async fn run(config: Config) -> Result<(), ForwardError> {
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                tracing::info!("Ctrl-C received; shutting down");
                cancel.cancel();
            }
        });
    }
    run_with_cancel(config, cancel).await
}

pub async fn run_with_cancel(
    config: Config,
    cancel: CancellationToken,
) -> Result<(), ForwardError> {
    let listener = TcpListener::bind(&config.bind)
        .await
        .map_err(|e| ForwardError::Bind {
            addr: config.bind.clone(),
            source: e,
        })?;
    run_with_listener(config, listener, cancel).await
}

pub async fn run_with_listener(
    config: Config,
    listener: TcpListener,
    cancel: CancellationToken,
) -> Result<(), ForwardError> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let ca = Ca::load_or_generate(&config.ca_dir)?;
    let acceptor = build_acceptor(&ca, config.intercept_host.clone())?;
    let extra_roots = load_extra_direct_roots(config.direct_extra_ca.as_deref())?;
    let clients = UpstreamClients::build(&extra_roots, pinned_sni(&config))?;
    let health = ProxyHealth::unknown();
    let tracker = TaskTracker::new();

    spawn_health_prober(&config, &health, &cancel, &tracker);
    log_listening(&config, &listener);
    warn_if_reachable_beyond_loopback(&listener);

    let ctx = build_mitm_context(&config, acceptor, clients, health, &tracker);

    accept_loop(&listener, &cancel, &tracker, &ctx).await;
    drain_connections(&tracker).await;
    Ok(())
}

fn pinned_sni(config: &Config) -> Option<&str> {
    config
        .direct_target
        .as_ref()
        .map(|_| config.intercept_host.as_str())
}

fn spawn_health_prober(
    config: &Config,
    health: &ProxyHealth,
    cancel: &CancellationToken,
    tracker: &TaskTracker,
) {
    let health = health.clone();
    let url = config.health_url.clone();
    let routed_authority = config.upstream_authority.to_string();
    let interval = config.probe_interval;
    let timeout = config.probe_timeout;
    let cancel = cancel.clone();
    tracker.spawn(async move {
        run_prober(health, url, routed_authority, interval, timeout, cancel).await;
    });
}

fn log_listening(config: &Config, listener: &TcpListener) {
    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| config.bind.clone());
    tracing::info!(
        bind = %local_addr,
        upstream = %config.upstream,
        intercept_host = %config.intercept_host,
        health_url = %config.health_url,
        "proxy-forward listening"
    );
}

fn warn_if_reachable_beyond_loopback(listener: &TcpListener) {
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    if is_reachable_beyond_loopback(&addr) {
        tracing::warn!(
            bind = %addr,
            "proxy-forward accepts connections from other machines and tunnels to any address, \
             including services bound to this machine's loopback; bind 127.0.0.1 unless that is \
             intended"
        );
    }
}

fn is_reachable_beyond_loopback(addr: &std::net::SocketAddr) -> bool {
    !addr.ip().is_loopback()
}

fn build_mitm_context(
    config: &Config,
    acceptor: TlsAcceptor,
    clients: UpstreamClients,
    health: ProxyHealth,
    tracker: &TaskTracker,
) -> MitmContext {
    MitmContext {
        acceptor,
        clients,
        health,
        upstream_authority: config.upstream_authority.clone(),
        intercept_host: Arc::from(config.intercept_host.as_str()),
        direct_target: config.direct_target.as_deref().map(Arc::from),
        max_body_bytes: config.max_body_bytes,
        tracker: tracker.clone(),
    }
}

async fn accept_loop(
    listener: &TcpListener,
    cancel: &CancellationToken,
    tracker: &TaskTracker,
    ctx: &MitmContext,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("accept loop stopping");
                break;
            }
            accepted = listener.accept() => {
                spawn_connection(accepted, tracker, ctx);
            }
        }
    }
}

fn spawn_connection(
    accepted: std::io::Result<(TcpStream, std::net::SocketAddr)>,
    tracker: &TaskTracker,
    ctx: &MitmContext,
) {
    let (stream, peer) = match accepted {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!(error = %e, "accept failed");
            return;
        }
    };
    let ctx = ctx.clone();
    tracker.spawn(async move {
        if let Err(e) = crate::connect::handle(stream, ctx).await {
            tracing::debug!(error = %e, %peer, "connection handler ended with error");
        }
    });
}

async fn drain_connections(tracker: &TaskTracker) {
    tracker.close();
    match tokio::time::timeout(SHUTDOWN_GRACE, tracker.wait()).await {
        Ok(()) => log_drained(),
        Err(_) => log_drain_grace_elapsed(),
    }
}

fn log_drained() {
    tracing::info!("all connections drained; exiting");
}

fn log_drain_grace_elapsed() {
    tracing::warn!("shutdown grace elapsed with connections still in flight");
}

#[cfg(test)]
mod tests {
    use super::is_reachable_beyond_loopback;

    fn addr(text: &str) -> Result<std::net::SocketAddr, std::net::AddrParseError> {
        text.parse()
    }

    #[test]
    fn loopback_binds_are_not_reachable_from_other_machines() -> Result<(), std::net::AddrParseError>
    {
        assert!(!is_reachable_beyond_loopback(&addr("127.0.0.1:9987")?));
        assert!(!is_reachable_beyond_loopback(&addr("[::1]:9987")?));
        Ok(())
    }

    #[test]
    fn wildcard_and_lan_binds_are_reachable_from_other_machines()
    -> Result<(), std::net::AddrParseError> {
        assert!(is_reachable_beyond_loopback(&addr("0.0.0.0:9987")?));
        assert!(is_reachable_beyond_loopback(&addr("[::]:9987")?));
        assert!(is_reachable_beyond_loopback(&addr("192.168.1.20:9987")?));
        Ok(())
    }
}
