use std::sync::Arc;

use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio_rustls::TlsAcceptor;
use tokio_util::task::TaskTracker;

use crate::error::ForwardError;
use crate::health::ProxyHealth;
use crate::upstream::UpstreamClients;

#[derive(Clone)]
pub struct MitmContext {
    pub acceptor: TlsAcceptor,
    pub clients: UpstreamClients,
    pub health: ProxyHealth,
    pub upstream_authority: http::uri::Authority,
    pub intercept_host: Arc<str>,
    pub direct_target: Option<Arc<str>>,
    pub max_body_bytes: usize,
    pub tracker: TaskTracker,
}

pub async fn serve<S>(client: S, ctx: MitmContext) -> Result<(), ForwardError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let tls = ctx
        .acceptor
        .accept(client)
        .await
        .map_err(|e| ForwardError::TlsAccept(e.to_string()))?;

    let clients = ctx.clients.clone();
    let health = ctx.health.clone();
    let upstream_authority = ctx.upstream_authority.clone();
    let intercept_host = Arc::clone(&ctx.intercept_host);
    let direct_target = ctx.direct_target.clone();
    let max_body_bytes = ctx.max_body_bytes;
    let tracker = ctx.tracker.clone();

    let service = service_fn(move |req| {
        let clients = clients.clone();
        let health = health.clone();
        let upstream_authority = upstream_authority.clone();
        let intercept_host = Arc::clone(&intercept_host);
        let direct_target = direct_target.clone();
        let tracker = tracker.clone();
        async move {
            let ctx = crate::route::DispatchCtx {
                health: &health,
                clients: &clients,
                upstream_authority: &upstream_authority,
                intercept_host: &intercept_host,
                direct_target: direct_target.as_deref(),
                max_body_bytes,
                tracker: &tracker,
            };
            let resp = crate::route::dispatch(req, ctx).await;
            Ok::<_, std::convert::Infallible>(resp)
        }
    });

    let io = TokioIo::new(tls);
    if let Err(e) = auto::Builder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(io, service)
        .await
    {
        tracing::debug!(error = %e, "MITM connection serve ended");
    }
    Ok(())
}
