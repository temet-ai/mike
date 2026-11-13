use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::streams::PendingFrame;

pub type BodySender = mpsc::Sender<Result<Bytes, std::io::Error>>;

#[derive(Debug, Clone, Copy)]
pub enum StreamEnd {
    Eof,
    ClientDisconnect,
    UpstreamError,
    Shutdown,
}

impl StreamEnd {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eof => "closed_eof",
            Self::ClientDisconnect => "closed_client_disconnect",
            Self::UpstreamError => "closed_upstream_error",
            Self::Shutdown => "closed_shutdown",
        }
    }
}

#[derive(Default)]
struct Boundary {
    line_has_bytes: bool,
    after_cr: bool,
    boundary: bool,
}

impl Boundary {
    fn new() -> Self {
        Self {
            boundary: true,
            ..Self::default()
        }
    }

    fn advance(&mut self, byte: u8) {
        if byte == b'\n' && self.after_cr {
            self.after_cr = false;
            return;
        }
        self.after_cr = byte == b'\r';
        if byte == b'\r' || byte == b'\n' {
            self.boundary = !self.line_has_bytes;
            self.line_has_bytes = false;
        } else {
            self.boundary = false;
            self.line_has_bytes = true;
        }
    }

    fn can_insert(&self) -> bool {
        self.boundary && !self.after_cr
    }
}

struct Relay {
    output: BodySender,
    injections: mpsc::Receiver<PendingFrame>,
    shutdown: CancellationToken,
    boundary: Boundary,
    pending: Option<PendingFrame>,
    injections_open: bool,
}

pub async fn relay<S>(
    mut upstream: S,
    output: BodySender,
    injections: mpsc::Receiver<PendingFrame>,
    shutdown: CancellationToken,
) -> StreamEnd
where
    S: Stream<Item = reqwest::Result<Bytes>> + Unpin,
{
    let mut relay = Relay {
        output,
        injections,
        shutdown,
        boundary: Boundary::new(),
        pending: None,
        injections_open: true,
    };
    loop {
        let deadline = relay.pending.as_ref().map(|frame| frame.deadline);
        tokio::select! {
            () = relay.output.closed() => return StreamEnd::ClientDisconnect,
            () = relay.shutdown.cancelled() => return StreamEnd::Shutdown,
            () = wait_deadline(deadline), if relay.pending.is_some() => {
                reject(relay.pending.take());
            }
            frame = relay.injections.recv(), if relay.injections_open && relay.pending.is_none() => {
                relay.injections_open = frame.is_some();
                relay.pending = frame;
                if relay.boundary.can_insert() {
                    deliver(&relay.output, relay.pending.take()).await;
                }
            }
            chunk = upstream.next() => {
                match chunk {
                    None => return StreamEnd::Eof,
                    Some(Err(err)) => {
                        let _ = relay.output.try_send(Err(std::io::Error::other(err.to_string())));
                        return StreamEnd::UpstreamError;
                    }
                    Some(Ok(bytes)) => {
                        if let Err(ended) = relay.forward_chunk(bytes).await { return ended; }
                    }
                }
            }
        }
    }
}

async fn wait_deadline(deadline: Option<tokio::time::Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    }
}

fn reject(frame: Option<PendingFrame>) {
    if let Some(frame) = frame {
        let _ = frame.result.send(false);
    }
}

async fn deliver(output: &BodySender, frame: Option<PendingFrame>) {
    let Some(frame) = frame else { return };
    if frame.result.is_closed() || frame.deadline <= tokio::time::Instant::now() {
        let _ = frame.result.send(false);
        return;
    }
    let sent = tokio::time::timeout_at(frame.deadline, output.send(Ok(frame.bytes)))
        .await
        .is_ok_and(|result| result.is_ok());
    let _ = frame.result.send(sent);
}

impl Relay {
    async fn forward_chunk(&mut self, bytes: Bytes) -> Result<(), StreamEnd> {
        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            self.boundary.advance(*byte);
            if self.pending.is_some() && self.boundary.can_insert() {
                self.send_upstream(bytes.slice(start..=index)).await?;
                start = index + 1;
                deliver(&self.output, self.pending.take()).await;
            }
        }
        if start < bytes.len() {
            self.send_upstream(bytes.slice(start..)).await?;
        }
        if self.boundary.can_insert() {
            deliver(&self.output, self.pending.take()).await;
        }
        Ok(())
    }

    async fn send_upstream(&mut self, bytes: Bytes) -> Result<(), StreamEnd> {
        let send = self.output.send(Ok(bytes));
        tokio::pin!(send);
        loop {
            let deadline = self.pending.as_ref().map(|frame| frame.deadline);
            tokio::select! {
                result = &mut send => return result.map_err(|_| StreamEnd::ClientDisconnect),
                () = self.shutdown.cancelled() => return Err(StreamEnd::Shutdown),
                () = wait_deadline(deadline), if self.pending.is_some() => reject(self.pending.take()),
                frame = self.injections.recv(), if self.injections_open && self.pending.is_none() => {
                    self.injections_open = frame.is_some();
                    self.pending = frame;
                }
            }
        }
    }
}
