use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Bytes;
use futures::Stream;
use mike_codex::responses_body::BodyCapture;

pub(super) type SharedCapture = Arc<Mutex<BodyCapture>>;
type Incoming = Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>;

pub(super) struct RequestBody {
    incoming: Incoming,
    capture: SharedCapture,
    expected: Option<u64>,
}

impl RequestBody {
    pub(super) fn new(incoming: Incoming, capture: SharedCapture, expected: Option<u64>) -> Self {
        Self {
            incoming,
            capture,
            expected,
        }
    }
}

impl Stream for RequestBody {
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.incoming.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                if let Ok(mut capture) = self.capture.lock() {
                    capture.push(&chunk);
                    capture.complete = self.expected == Some(capture.received);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(None) => {
                if let Ok(mut capture) = self.capture.lock() {
                    capture.complete = true;
                }
                Poll::Ready(None)
            }
            other => other,
        }
    }
}
