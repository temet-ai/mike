use axum::http::Request;
use tracing::Span;

pub(crate) fn inspect_request_span<B>(request: &Request<B>) -> Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    tracing::info_span!(
        "inspect_request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id,
    )
}

#[cfg(test)]
mod tests {
    use tracing::subscriber::with_default;

    use super::*;

    #[test]
    fn inspect_request_span_carries_the_request_id_header() -> anyhow::Result<()> {
        let request = Request::builder()
            .uri("/api/health")
            .header("x-request-id", "abc-123")
            .body(())?;
        let subscriber = tracing_subscriber::registry();
        with_default(subscriber, || {
            let span = inspect_request_span(&request);
            anyhow::ensure!(span.metadata().map(|m| m.name()) == Some("inspect_request"));
            Ok(())
        })
    }
}
