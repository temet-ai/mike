use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use futures::Stream;

use super::ResponsesState;
use super::record::{ExchangeRecord, ExchangeStatus, Finished, spawn_finish};

type Upstream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

pub(super) struct Tee {
    upstream: Upstream,
    captured: BytesMut,
    pending: Option<(ResponsesState, ExchangeRecord)>,
}

impl Tee {
    pub(super) fn new(upstream: Upstream, state: ResponsesState, record: ExchangeRecord) -> Self {
        Self {
            upstream,
            captured: BytesMut::new(),
            pending: Some((state, record)),
        }
    }

    fn finish(&mut self, status: ExchangeStatus, error: Option<String>) {
        if let Some((state, record)) = self.pending.take() {
            let finished = Finished {
                status,
                response_body: Some(self.captured.split().freeze()),
                error,
            };
            spawn_finish(state, record, finished);
        }
    }
}

impl Stream for Tee {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.upstream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.captured.extend_from_slice(&chunk);
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(error))) => {
                let message = format!("upstream stream failed: {error}");
                self.finish(ExchangeStatus::Error, Some(message.clone()));
                Poll::Ready(Some(Err(std::io::Error::other(message))))
            }
            Poll::Ready(None) => {
                self.finish(ExchangeStatus::Complete, None);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for Tee {
    fn drop(&mut self) {
        self.finish(
            ExchangeStatus::Aborted,
            Some("client went away before the response ended".to_string()),
        );
    }
}
