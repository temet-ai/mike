use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use proxy_forward::config::{Config, ProbeMode};
use rustls_pki_types::CertificateDer;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::common::{StubHandle, install_crypto, mint_cert};

pub(crate) const INTERCEPT_HOST: &str = "api.anthropic.com";

pub(crate) struct RunningProxy {
    pub(crate) addr: SocketAddr,
    pub(crate) ca_der: CertificateDer<'static>,
    pub(crate) cancel: CancellationToken,
    _ca_dir: TempDir,
}

pub(crate) fn named_temp_dir(tag: &str) -> Result<TempDir> {
    tempfile::Builder::new()
        .prefix(&format!("pf-it-{tag}-"))
        .tempdir()
        .context("create temp dir")
}

pub(crate) async fn spawn_forward(
    upstream: String,
    health_url: String,
    direct_extra_ca: Option<PathBuf>,
    direct_target: Option<String>,
    probe_interval: Duration,
) -> Result<RunningProxy> {
    spawn_forward_capped(
        upstream,
        health_url,
        direct_extra_ca,
        direct_target,
        probe_interval,
        proxy_forward::config::DEFAULT_MAX_BODY_BYTES,
    )
    .await
}

pub(crate) async fn spawn_forward_capped(
    upstream: String,
    health_url: String,
    direct_extra_ca: Option<PathBuf>,
    direct_target: Option<String>,
    probe_interval: Duration,
    max_body_bytes: usize,
) -> Result<RunningProxy> {
    install_crypto();
    let ca_dir = named_temp_dir("ca")?;

    let upstream_authority = upstream
        .parse::<http::Uri>()
        .ok()
        .and_then(|u| u.authority().cloned())
        .ok_or_else(|| anyhow!("test upstream {upstream:?} has no authority"))?;

    let config = Config {
        bind: "127.0.0.1:0".to_string(),
        upstream,
        upstream_authority,
        intercept_host: INTERCEPT_HOST.to_string(),
        probe_interval,
        probe_timeout: Duration::from_millis(250),
        probe_mode: ProbeMode::Http,
        health_url,
        ca_dir: ca_dir.path().to_path_buf(),
        direct_extra_ca,
        direct_target,
        allow_insecure_direct: true,
        max_body_bytes,
    };

    let listener = TcpListener::bind(&config.bind)
        .await
        .context("bind forward")?;
    let addr = listener.local_addr().context("forward addr")?;

    let cancel = CancellationToken::new();
    let run_cancel = cancel.clone();
    let run_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = proxy_forward::run_with_listener(run_config, listener, run_cancel).await {
            eprintln!("run_with_listener exited with error: {e:#}");
        }
    });

    let ca_path = ca_dir.path().join("ca-cert.pem");
    let ca_der = wait_for_ca(&ca_path).await?;

    Ok(RunningProxy {
        addr,
        ca_der,
        cancel,
        _ca_dir: ca_dir,
    })
}

async fn wait_for_ca(ca_path: &std::path::Path) -> Result<CertificateDer<'static>> {
    use rustls_pki_types::pem::PemObject;

    for _ in 0..500 {
        if ca_path.is_file()
            && let Ok(cert) = CertificateDer::from_pem_file(ca_path)
        {
            return Ok(cert);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(anyhow!("forward proxy never wrote its CA cert"))
}

pub(crate) async fn wait_until_probed(health: &StubHandle) -> Result<()> {
    for _ in 0..400 {
        if health.received.lock().await.len() >= 2 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(anyhow!(
        "health stub was never probed twice — router never settled"
    ))
}

pub(crate) fn closed_health_url() -> String {
    "http://127.0.0.1:1/api/health".to_string()
}

pub(crate) fn closed_direct_target() -> String {
    "127.0.0.1:1".to_string()
}

pub(crate) fn write_extra_ca(pem: &str) -> Result<(TempDir, PathBuf)> {
    let dir = named_temp_dir("extraca")?;
    let path = dir.path().join("anthropic-ca.pem");
    std::fs::write(&path, pem.as_bytes()).context("write extra ca")?;
    Ok((dir, path))
}

pub(crate) fn dummy_extra_ca() -> Result<(TempDir, PathBuf)> {
    write_extra_ca(&mint_cert("dummy-extra-ca.test")?.ca_pem)
}
