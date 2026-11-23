use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Bytes;
use futures::Stream;

use super::spool::{BodyFile, BodyWriter};

pub type SharedCapture = Arc<BodyFile>;
type Incoming = Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>;

pub struct RequestBody {
    incoming: Incoming,
    writer: BodyWriter,
    ended: bool,
}

impl RequestBody {
    pub fn new(incoming: Incoming, writer: BodyWriter) -> Self {
        Self {
            incoming,
            writer,
            ended: false,
        }
    }
}

impl Stream for RequestBody {
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.writer.poll_written(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(result)) => {
                    return Poll::Ready(Some(result.map_err(axum::Error::new)));
                }
                Poll::Ready(None) => {}
            }
            if self.ended {
                return Poll::Ready(None);
            }
            match self.incoming.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if let Err(error) = self.writer.submit(chunk) {
                        self.ended = true;
                        self.writer.finish(false);
                        return Poll::Ready(Some(Err(axum::Error::new(error))));
                    }
                }
                Poll::Ready(None) => {
                    self.ended = true;
                    self.writer.finish(true);
                }
                Poll::Ready(Some(Err(error))) => {
                    self.ended = true;
                    self.writer.finish(false);
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
