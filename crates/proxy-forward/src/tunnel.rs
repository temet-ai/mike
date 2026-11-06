use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

use crate::error::ForwardError;

pub async fn blind<S>(mut client: S, host: &str, port: u16) -> Result<(), ForwardError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut upstream = TcpStream::connect((host, port))
        .await
        .map_err(|e| ForwardError::UpstreamConnect(format!("tunnel dial {host}:{port}: {e}")))?;

    match copy_bidirectional(&mut client, &mut upstream).await {
        Ok((from_client, from_upstream)) => {
            log_tunnel_closed(host, port, from_client, from_upstream)
        }
        Err(e) => log_tunnel_copy_ended(host, port, &e),
    }
    Ok(())
}

fn log_tunnel_closed(host: &str, port: u16, from_client: u64, from_upstream: u64) {
    tracing::info!(host, port, from_client, from_upstream, "tunnel closed");
}

fn log_tunnel_copy_ended(host: &str, port: u16, e: &std::io::Error) {
    tracing::info!(error = %e, host, port, "tunnel copy ended");
}
