use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use futures::Stream;

use super::record::{Finished, Recorder, SessionlessRecord, Status};

type Upstream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

pub struct Tee {
    upstream: Upstream,
    kept: Kept,
    pending: Option<(Recorder, SessionlessRecord)>,
}

#[derive(Debug, Default)]
pub struct Kept {
    pub bytes: BytesMut,
    pub size: u64,
    pub cap: u64,
    pub refused: bool,
}

impl Kept {
    pub fn take(&mut self, chunk: &[u8]) {
        self.size = self.size.saturating_add(chunk.len() as u64);
        if self.refused {
            return;
        }
        if self.cap > 0 && self.size > self.cap {
            self.refused = true;
            self.bytes = BytesMut::new();
            return;
        }
        self.bytes.extend_from_slice(chunk);
    }

    fn body(&mut self) -> Option<Bytes> {
        (!self.refused).then(|| self.bytes.split().freeze())
    }
}

impl Tee {
    pub fn new(
        upstream: Upstream,
        cap: u64,
        recorder: Recorder,
        record: SessionlessRecord,
    ) -> Self {
        Self {
            upstream,
            kept: Kept {
                cap,
                ..Kept::default()
            },
            pending: Some((recorder, record)),
        }
    }

    fn finish(&mut self, status: Status, error: Option<String>) {
        if let Some((recorder, record)) = self.pending.take() {
            let finished = Finished {
                status,
                response_body: self.kept.body(),
                response_body_size: Some(self.kept.size),
                refused: self.kept.refused,
                error,
            };
            recorder.record(record, finished);
        }
    }
}

impl Stream for Tee {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.upstream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.kept.take(&chunk);
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(error))) => {
                let message = format!("upstream stream failed: {error}");
                self.finish(Status::Error, Some(message.clone()));
                Poll::Ready(Some(Err(std::io::Error::other(message))))
            }
            Poll::Ready(None) => {
                self.finish(Status::Complete, None);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for Tee {
    fn drop(&mut self) {
        self.finish(
            Status::Aborted,
            Some("client went away before the response ended".to_string()),
        );
    }
}
