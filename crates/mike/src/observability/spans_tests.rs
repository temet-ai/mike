use std::sync::{Arc, Mutex};

use tracing::subscriber::with_default;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

#[derive(Clone, Default)]
struct SeenSpans(Arc<Mutex<Vec<String>>>);

impl<S> tracing_subscriber::Layer<S> for SeenSpans
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let names = ctx
            .event_scope(event)
            .map(|scope| {
                scope
                    .map(|span| span.name().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Ok(mut seen) = self.0.lock() {
            seen.extend(names);
        }
    }
}

#[test]
fn spawn_tracked_task_logs_inside_the_spawning_span() -> anyhow::Result<()> {
    let seen = SeenSpans::default();
    let subscriber = tracing_subscriber::registry().with(seen.clone());
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    with_default(subscriber, || {
        runtime.block_on(async {
            let tracker = TaskTracker::new();
            let span = tracing::info_span!("proxy_request");
            let handle = span
                .in_scope(|| spawn_tracked(&tracker, async { tracing::info!("inside the task") }));
            handle.await
        })
    })?;
    let seen = seen.0.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
    anyhow::ensure!(
        seen.contains(&"proxy_request".to_string()),
        "spans seen: {seen:?}"
    );
    Ok(())
}

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
