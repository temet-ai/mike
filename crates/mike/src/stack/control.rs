use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;

use super::instance::{DaemonOwner, InstanceSpec};
use super::protocol::{self, Decision, InstanceIdentity, Request, Response, RuntimeStatus};

const CONTROL_DEADLINE: Duration = Duration::from_secs(2);

struct ServerState {
    identity: InstanceIdentity,
    config_file: std::path::PathBuf,
    fingerprint: String,
    persist_enabled: bool,
    ready: AtomicBool,
    bound_listeners: std::sync::OnceLock<Vec<std::net::SocketAddr>>,
    shutdown: CancellationToken,
}

pub(crate) struct ControlServer {
    state: Arc<ServerState>,
    task: tokio::task::JoinHandle<()>,
    socket: std::path::PathBuf,
    socket_inode: u64,
}

impl ControlServer {
    #[cfg(unix)]
    pub(crate) fn bind(owner: &DaemonOwner, shutdown: CancellationToken) -> Result<Self> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let socket = owner.spec.socket_path();
        remove_stale_socket(&socket, owner.stale_socket_is_owned)?;
        let listener = tokio::net::UnixListener::bind(&socket)
            .context("cannot bind the selected installation's private control socket; Unix sockets and a short private state path are required")?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let socket_inode = std::fs::symlink_metadata(&socket)?.ino();
        let state = Arc::new(ServerState {
            identity: owner.identity.clone(),
            config_file: owner.spec.paths.config_file.clone(),
            fingerprint: owner.spec.fingerprint.clone(),
            persist_enabled: owner.spec.persist_enabled,
            ready: AtomicBool::new(false),
            bound_listeners: std::sync::OnceLock::new(),
            shutdown,
        });
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let handled =
                    tokio::time::timeout(CONTROL_DEADLINE, handle(&mut stream, &task_state)).await;
                if !matches!(handled, Ok(Ok(()))) {
                    tracing::debug!("private daemon control request rejected or timed out");
                }
            }
        });
        Ok(Self {
            state,
            task,
            socket,
            socket_inode,
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn bind(_owner: &DaemonOwner, _shutdown: CancellationToken) -> Result<Self> {
        anyhow::bail!("private daemon control requires Unix sockets")
    }

    pub(crate) fn mark_ready(&self, addresses: Vec<std::net::SocketAddr>) -> Result<()> {
        self.state
            .bound_listeners
            .set(addresses)
            .map_err(|_| anyhow::anyhow!("daemon listener facts were already published"))?;
        self.state.ready.store(true, Ordering::Release);
        Ok(())
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.task.abort();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if std::fs::symlink_metadata(&self.socket)
                .is_ok_and(|metadata| metadata.ino() == self.socket_inode)
            {
                let _ = std::fs::remove_file(&self.socket);
            }
        }
    }
}

#[cfg(unix)]
async fn handle(stream: &mut tokio::net::UnixStream, state: &ServerState) -> Result<()> {
    require_same_user(stream)?;
    let request: Request = protocol::read(stream).await?;
    let decision = Decision {
        identity: &state.identity,
        config_file: &state.config_file,
        boot_fingerprint: &state.fingerprint,
        ready: state.ready.load(Ordering::Acquire),
        stopping: state.shutdown.is_cancelled(),
        persist_enabled: state.persist_enabled,
        bound_listeners: state.bound_listeners.get().map(Vec::as_slice),
    };
    let (response, stop) = decision.reply(&request);
    if stop {
        let peer_pid = stream.peer_cred().ok().and_then(|c| c.pid());
        tracing::warn!(
            peer_pid,
            "control socket: a cooperative stop was requested by another process; shutting down"
        );
        state.shutdown.cancel();
    }
    protocol::write(stream, &response).await
}

#[cfg(unix)]
fn require_same_user(stream: &tokio::net::UnixStream) -> Result<()> {
    let credentials = stream
        .peer_cred()
        .context("cannot verify private daemon control peer")?;
    anyhow::ensure!(
        credentials.uid() == unsafe { libc::geteuid() },
        "private daemon control peer has a different owner"
    );
    Ok(())
}

#[cfg(unix)]
fn socket_metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect private daemon control socket"),
    };
    anyhow::ensure!(
        metadata.file_type().is_socket()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.permissions().mode() & 0o077 == 0,
        "private daemon control path is not a private socket owned by this user; nothing was removed"
    );
    Ok(Some(metadata))
}

#[cfg(unix)]
fn remove_stale_socket(path: &Path, prior_identity_matches: bool) -> Result<()> {
    if socket_metadata(path)?.is_some() {
        anyhow::ensure!(
            prior_identity_matches,
            "an unverified socket occupies the control path; no socket was removed"
        );
        std::fs::remove_file(path).context("cannot remove stale owned control socket")?;
    }
    Ok(())
}

pub(crate) async fn status(
    spec: &InstanceSpec,
    compare_boot: bool,
) -> Result<Option<RuntimeStatus>> {
    let request = Request::status(
        spec.paths.installation_id.clone(),
        compare_boot.then(|| spec.fingerprint.clone()),
    );
    let status = request_status(spec, &request).await?;
    if let Some(status) = &status {
        super::instance::verify_owner(spec, &status.identity)?;
    }
    Ok(status)
}

pub(crate) async fn stop(
    spec: &InstanceSpec,
    identity: &InstanceIdentity,
) -> Result<RuntimeStatus> {
    request_status(spec, &Request::stop(identity))
        .await?
        .context("selected daemon control socket disappeared before stop")
}

#[cfg(unix)]
async fn request_status(spec: &InstanceSpec, request: &Request) -> Result<Option<RuntimeStatus>> {
    if socket_metadata(&spec.socket_path())?.is_none() {
        return Ok(None);
    }
    let exchange = async {
        let mut stream = tokio::net::UnixStream::connect(spec.socket_path())
            .await
            .context("cannot connect to private daemon control socket")?;
        require_same_user(&stream)?;
        protocol::write(&mut stream, request).await?;
        let response: Response = protocol::read(&mut stream).await?;
        let status = response.into_status_for(&spec.paths.config_file)?;
        anyhow::ensure!(
            status.identity.instance_id == spec.paths.installation_id,
            "private control response identifies a different installation"
        );
        Ok(Some(status))
    };
    tokio::time::timeout(CONTROL_DEADLINE, exchange)
        .await
        .context("private daemon control timed out")?
}

#[cfg(not(unix))]
async fn request_status(_spec: &InstanceSpec, _request: &Request) -> Result<Option<RuntimeStatus>> {
    anyhow::bail!("private daemon control requires Unix sockets")
}
