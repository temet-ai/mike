use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use mike_codex::responses::{MAX_FRAME_BYTES, ResponsesStream, ResponsesSummary, summarize_json};
use mike_codex::responses_body::BodyCapture;

use super::ResponsesState;
use super::record::{ExchangeRecord, ExchangeStatus, Finished, spawn_finish};
use super::spool::BodyWriter;

type Upstream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

pub struct Tee {
    upstream: Upstream,
    writer: BodyWriter,
    json: BodyCapture,
    stream: Option<ResponsesStream>,
    expected: Option<u64>,
    pending: Option<(ResponsesState, ExchangeRecord)>,
}

impl Tee {
    pub fn new(
        upstream: Upstream,
        state: ResponsesState,
        record: ExchangeRecord,
        expected: Option<u64>,
        writer: BodyWriter,
    ) -> Self {
        Self {
            upstream,
            writer,
            json: BodyCapture::new(if record.response_is_event_stream {
                0
            } else {
                MAX_FRAME_BYTES
            }),
            stream: record
                .response_is_event_stream
                .then(ResponsesStream::default),
            expected,
            pending: Some((state, record)),
        }
    }

    fn summary(&mut self) -> ResponsesSummary {
        if let Some(stream) = self.stream.take() {
            stream.finish()
        } else if let Some(body) = self.json.body() {
            summarize_json(body)
        } else {
            ResponsesSummary {
                oversized_frames: usize::from(self.json.refused),
                ..Default::default()
            }
        }
    }

    fn finish(&mut self, status: ExchangeStatus, error: Option<String>) {
        self.writer.finish(status == ExchangeStatus::Complete);
        if let Some((state, record)) = self.pending.take() {
            let summary = self.summary();
            let complete =
                record.response_body.complete.load(Ordering::Acquire) || summary.terminal;
            let status = if status == ExchangeStatus::Aborted && complete {
                ExchangeStatus::Complete
            } else {
                status
            };
            let error = if complete && status == ExchangeStatus::Complete {
                None
            } else {
                error
            };
            spawn_finish(
                state,
                record,
                Finished {
                    status,
                    summary,
                    error,
                },
            );
        }
    }

    fn observe(&mut self, chunk: &[u8]) {
        self.json.push(chunk);
        if let Some((_, record)) = &self.pending {
            let complete =
                self.expected == Some(record.response_body.received.load(Ordering::Acquire));
            if complete {
                record.response_body.complete.store(true, Ordering::Release);
            }
            self.json.complete = complete;
        }
        if let Some(stream) = &mut self.stream {
            stream.push(chunk);
        }
    }
}

impl Stream for Tee {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.writer.poll_written(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => {
                    self.observe(&chunk);
                    return Poll::Ready(Some(Ok(chunk)));
                }
                Poll::Ready(Some(Err(error))) => {
                    self.finish(
                        ExchangeStatus::Error,
                        Some(format!("response capture failed: {error}")),
                    );
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => {}
            }
            if self.pending.is_none() {
                return Poll::Ready(None);
            }
            match self.upstream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if let Err(error) = self.writer.submit(chunk) {
                        self.finish(
                            ExchangeStatus::Error,
                            Some(format!("response capture failed: {error}")),
                        );
                        return Poll::Ready(Some(Err(error)));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    let message = format!("upstream stream failed: {error}");
                    self.finish(ExchangeStatus::Error, Some(message.clone()));
                    return Poll::Ready(Some(Err(std::io::Error::other(message))));
                }
                Poll::Ready(None) => {
                    self.json.complete = true;
                    self.finish(ExchangeStatus::Complete, None);
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl Drop for Tee {
    fn drop(&mut self) {
        self.finish(
            ExchangeStatus::Aborted,
            Some("client went away before the response ended".into()),
        );
    }
}
