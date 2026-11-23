use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::{OnceCell, mpsc, oneshot};
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::store::persistence::runtime::capture::CaptureExchangeInput;

#[derive(Debug)]
pub struct Spool {
    pub id: Uuid,
    pub directory: PathBuf,
    _lock: std::fs::File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub exchange: CaptureExchangeInput,
    #[serde(default)]
    pub identity: IdentityInput,
    pub request_predecessor: Option<i64>,
    pub request_complete: bool,
    pub response_complete: bool,
    pub response_received: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityInput {
    pub launch: Option<Uuid>,
    pub home: Option<PathBuf>,
    pub thread: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug)]
pub struct BodyFile {
    pub path: PathBuf,
    pub received: AtomicU64,
    pub complete: AtomicBool,
    done: OnceCell<Result<(), String>>,
    changed: tokio::sync::Notify,
}

struct Write {
    chunk: Bytes,
    acknowledged: oneshot::Sender<Result<Bytes, String>>,
}

pub struct BodyWriter {
    sender: Option<mpsc::Sender<Write>>,
    pending: Option<oneshot::Receiver<Result<Bytes, String>>>,
    file: Arc<BodyFile>,
}

impl Spool {
    pub async fn create(root: &Path) -> std::io::Result<Arc<Self>> {
        let id = Uuid::new_v4();
        let directory = root.join(id.to_string());
        let preparing = root.join(format!(".creating-{id}"));
        private_directory(&preparing).await?;
        let lock = lock(&preparing)?.ok_or_else(|| std::io::Error::other("capture is locked"))?;
        tokio::fs::rename(&preparing, &directory).await?;
        Ok(Arc::new(Self {
            id,
            directory,
            _lock: lock,
        }))
    }

    pub async fn write_manifest(&self, manifest: &Manifest) -> Result<(), String> {
        write_manifest(&self.directory, manifest).await
    }
}

impl BodyFile {
    pub async fn start(
        spool: &Spool,
        name: &str,
        expected: Option<u64>,
        tasks: &TaskTracker,
    ) -> std::io::Result<(Arc<Self>, BodyWriter)> {
        let path = spool.directory.join(name);
        let output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await?;
        let file = Arc::new(Self {
            path,
            received: AtomicU64::new(0),
            complete: AtomicBool::new(expected == Some(0)),
            done: OnceCell::new(),
            changed: tokio::sync::Notify::new(),
        });
        let (sender, receiver) = mpsc::channel(1);
        let observed = file.clone();
        tasks.spawn(async move {
            let result = write_body(output, receiver, &observed, expected).await;
            if let Err(error) = &result {
                tracing::error!(path = %observed.path.display(), error, "Codex capture staging failed");
            }
            let _ = observed.done.set(result);
            observed.changed.notify_waiters();
        });
        let writer = BodyWriter {
            sender: Some(sender),
            pending: None,
            file: file.clone(),
        };
        Ok((file, writer))
    }

    pub async fn wait(&self) -> Result<(), String> {
        loop {
            let changed = self.changed.notified();
            if let Some(result) = self.done.get() {
                return result.clone();
            }
            changed.await;
        }
    }
}

impl BodyWriter {
    pub fn submit(&mut self, chunk: Bytes) -> std::io::Result<()> {
        if self.pending.is_some() {
            return Err(std::io::Error::other("capture write already pending"));
        }
        let (acknowledged, result) = oneshot::channel();
        self.sender
            .as_ref()
            .ok_or_else(|| std::io::Error::other("capture is closed"))?
            .try_send(Write {
                chunk,
                acknowledged,
            })
            .map_err(|error| {
                std::io::Error::other(format!("capture writer unavailable: {error}"))
            })?;
        self.pending = Some(result);
        Ok(())
    }

    pub fn poll_written(&mut self, cx: &mut Context<'_>) -> Poll<Option<std::io::Result<Bytes>>> {
        let Some(pending) = &mut self.pending else {
            return Poll::Ready(None);
        };
        let result = match pending.poll_unpin(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Ok(result)) => result.map_err(std::io::Error::other),
            Poll::Ready(Err(error)) => Err(std::io::Error::other(error)),
        };
        self.pending = None;
        Poll::Ready(Some(result))
    }

    pub fn finish(&mut self, complete: bool) {
        if complete {
            self.file.complete.store(true, Ordering::Release);
        }
        self.sender.take();
    }
}

async fn write_body(
    mut output: tokio::fs::File,
    mut receiver: mpsc::Receiver<Write>,
    body: &BodyFile,
    expected: Option<u64>,
) -> Result<(), String> {
    while let Some(write) = receiver.recv().await {
        let result = async {
            output.write_all(&write.chunk).await?;
            output.flush().await
        }
        .await;
        if let Err(error) = result {
            let message = error.to_string();
            let _ = write.acknowledged.send(Err(message.clone()));
            return Err(message);
        }
        let received = body
            .received
            .fetch_add(write.chunk.len() as u64, Ordering::AcqRel)
            + write.chunk.len() as u64;
        if expected == Some(received) {
            body.complete.store(true, Ordering::Release);
        }
        let _ = write.acknowledged.send(Ok(write.chunk));
    }
    output.sync_all().await.map_err(|error| error.to_string())
}

pub async fn write_manifest(directory: &Path, manifest: &Manifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    let temporary = directory.join("manifest.new");
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    tokio::fs::rename(temporary, directory.join("manifest.json"))
        .await
        .map_err(|error| error.to_string())
}

pub fn lock(directory: &Path) -> std::io::Result<Option<std::fs::File>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("capture.lock"))?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    tokio::fs::create_dir_all(path).await?;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await
}
