use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use mike_codex::responses::{MAX_FRAME_BYTES, ResponsesStream, ResponsesSummary, summarize_json};
use mike_codex::responses_body::BodyCapture;

use super::ResponsesState;
use super::record::{ExchangeRecord, ExchangeStatus, Finished, spawn_finish};

type Upstream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

pub struct Tee {
    upstream: Upstream,
    captured: BodyCapture,
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
    ) -> Self {
        let limit = if record.diagnostics_enabled {
            record
                .request_body
                .lock()
                .ok()
                .and_then(|capture| usize::try_from(capture.received).ok())
                .map(|received| super::MAX_DIAGNOSTIC_BYTES.saturating_sub(received))
                .unwrap_or(0)
        } else {
            0
        };
        let monitored = record.slot.is_some();
        Self {
            upstream,
            captured: BodyCapture::new(limit),
            json: BodyCapture::new(if monitored && !record.response_is_event_stream {
                MAX_FRAME_BYTES
            } else {
                0
            }),
            stream: (monitored && record.response_is_event_stream).then(ResponsesStream::default),
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
        if let Some((state, record)) = self.pending.take() {
            let summary = self.summary();
            let complete = self.captured.complete || summary.terminal;
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
            let finished = Finished {
                status,
                response_body: self.captured.body().map(Bytes::copy_from_slice),
                summary,
                evidence_complete: self.captured.complete,
                evidence_refused: self.captured.refused
                    || record.diagnostics_enabled
                        && record
                            .request_body
                            .lock()
                            .map(|request| {
                                request.received.saturating_add(self.captured.received)
                                    > super::MAX_DIAGNOSTIC_BYTES as u64
                            })
                            .unwrap_or(true),
                error,
            };
            spawn_finish(state, record, finished);
        }
    }

    fn observe(&mut self, chunk: &[u8]) {
        self.captured.push(chunk);
        self.json.push(chunk);
        let complete = self.expected == Some(self.captured.received);
        self.captured.complete = complete;
        self.json.complete = complete;
        if let Some(stream) = &mut self.stream {
            stream.push(chunk);
        }
    }
}

impl Stream for Tee {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.upstream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.observe(&chunk);
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(error))) => {
                let message = format!("upstream stream failed: {error}");
                self.finish(ExchangeStatus::Error, Some(message.clone()));
                Poll::Ready(Some(Err(std::io::Error::other(message))))
            }
            Poll::Ready(None) => {
                self.captured.complete = true;
                self.json.complete = true;
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
