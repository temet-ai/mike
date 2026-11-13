use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

pub fn tee_sse_response(
    upstream: reqwest::Response,
    tracker: &TaskTracker,
) -> (axum::body::Body, mpsc::Receiver<Bytes>) {
    tee_sse_stream(upstream.bytes_stream(), tracker)
}

fn tee_sse_stream<S, E>(
    stream: S,
    tracker: &TaskTracker,
) -> (axum::body::Body, mpsc::Receiver<Bytes>)
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let (parser_tx, parser_rx) = mpsc::channel::<Bytes>(64);

    tracker.spawn(Instrument::in_current_span(async move {
        futures::pin_mut!(stream);
        let mut parser_alive = true;
        loop {
            let chunk_result = tokio::select! {
                biased;
                _ = body_tx.closed() => break,
                chunk = stream.next() => chunk,
            };
            let Some(chunk_result) = chunk_result else {
                break;
            };
            match chunk_result {
                Ok(chunk) => {
                    if parser_alive {
                        let sent = tokio::select! {
                            biased;
                            _ = body_tx.closed() => break,
                            sent = parser_tx.send(chunk.clone()) => sent,
                        };
                        if sent.is_err() {
                            tracing::debug!(
                                "parser receiver dropped, continuing client forward only"
                            );
                            parser_alive = false;
                        }
                    }

                    if body_tx.send(Ok(chunk)).await.is_err() {
                        tracing::debug!("client disconnected, stopping SSE forwarder");
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "upstream SSE read error");
                    let _ = body_tx
                        .send(Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            e.to_string(),
                        )))
                        .await;
                    break;
                }
            }
        }
    }));

    let body = axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(body_rx));

    (body, parser_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn sse_upstream(body: &str) -> anyhow::Result<(MockServer, reqwest::Response)> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;

        let resp = reqwest::Client::new()
            .get(format!("{}/sse", server.uri()))
            .send()
            .await?;
        Ok((server, resp))
    }

    async fn collect_body(body: axum::body::Body) -> anyhow::Result<Bytes> {
        let bytes = axum::body::to_bytes(body, usize::MAX).await?;
        Ok(bytes)
    }

    #[tokio::test]
    async fn tee_forwards_full_body_to_client() -> anyhow::Result<()> {
        let payload = "event: message_start\ndata: {\"id\":\"abc\"}\n\n";
        let (_server, upstream) = sse_upstream(payload).await?;

        let tracker = TaskTracker::new();
        let (body, _parser_rx) = tee_sse_response(upstream, &tracker);

        let bytes = collect_body(body).await?;
        assert_eq!(bytes, payload.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn tee_delivers_chunks_to_parser_channel() -> anyhow::Result<()> {
        let payload = "event: a\ndata: 1\n\nevent: b\ndata: 2\n\n";
        let (_server, upstream) = sse_upstream(payload).await?;

        let tracker = TaskTracker::new();
        let (body, mut parser_rx) = tee_sse_response(upstream, &tracker);

        let _ = collect_body(body).await?;

        let mut seen = Vec::new();
        while let Some(chunk) = parser_rx.recv().await {
            seen.extend_from_slice(&chunk);
        }
        assert_eq!(seen, payload.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn tee_finishes_tracked_task_on_upstream_eof() -> anyhow::Result<()> {
        let (_server, upstream) = sse_upstream("event: done\ndata: {}\n\n").await?;

        let tracker = TaskTracker::new();
        let (body, _parser_rx) = tee_sse_response(upstream, &tracker);
        let _ = collect_body(body).await?;

        tracker.close();
        tokio::time::timeout(std::time::Duration::from_secs(5), tracker.wait()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn tee_client_drop_terminates_forwarder() -> anyhow::Result<()> {
        let payload = "x".repeat(8 * 1024);
        let (_server, upstream) = sse_upstream(&payload).await?;

        let tracker = TaskTracker::new();
        let (body, _parser_rx) = tee_sse_response(upstream, &tracker);

        drop(body);

        tracker.close();
        tokio::time::timeout(std::time::Duration::from_secs(5), tracker.wait()).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tee_cancellation_tests.rs"]
pub mod cancellation_tests;

#[cfg(test)]
#[path = "tee_retained_bytes_tests.rs"]
pub mod retained_bytes_tests;
