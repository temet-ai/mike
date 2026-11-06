use super::*;
use crate::store::persistence::PersistHandle;
use std::future::Future;
use std::sync::LazyLock;
use std::time::Duration;

pub(crate) const LIVE_PROBE_DEADLINE: Duration = Duration::from_millis(300);
pub(crate) const CAPABILITY_PROBE_DEADLINE: Duration = Duration::from_secs(2);

static PROBE_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProbeState {
    Ok,
    Failed,
    TimedOut,
    NotConfigured,
}

pub(crate) enum Bounded<T> {
    Done(T),
    TimedOut,
}

pub(crate) async fn bounded<T>(
    work: impl Future<Output = T>,
    deadline: impl Future<Output = ()>,
) -> Bounded<T> {
    tokio::select! {
        biased;
        value = work => Bounded::Done(value),
        () = deadline => Bounded::TimedOut,
    }
}

pub(crate) async fn within<T>(work: impl Future<Output = T>, limit: Duration) -> Bounded<T> {
    bounded(work, tokio::time::sleep(limit)).await
}

pub(crate) fn classify<T, E>(outcome: Bounded<Result<T, E>>) -> (ProbeState, Option<T>) {
    match outcome {
        Bounded::Done(Ok(value)) => (ProbeState::Ok, Some(value)),
        Bounded::Done(Err(_)) => (ProbeState::Failed, None),
        Bounded::TimedOut => (ProbeState::TimedOut, None),
    }
}

pub(crate) async fn probe_embedding_url(
    embedding_service: &EmbeddingService,
    limit: Duration,
) -> ProbeState {
    let probe_url = format!("{}/models", embedding_service.url());
    match within(PROBE_CLIENT.get(&probe_url).send(), limit).await {
        Bounded::Done(Ok(resp)) if resp.status().is_success() => ProbeState::Ok,
        Bounded::Done(_) => ProbeState::Failed,
        Bounded::TimedOut => ProbeState::TimedOut,
    }
}

pub(crate) async fn probe_embedding(
    embedding_service: &EmbeddingService,
    limit: Duration,
) -> ProbeState {
    if !embedding_service.is_available() {
        return ProbeState::NotConfigured;
    }
    probe_embedding_url(embedding_service, limit).await
}

pub(crate) async fn probe_persistence(
    persist: Option<&PersistHandle>,
    limit: Duration,
) -> ProbeState {
    let Some(persist) = persist else {
        return ProbeState::NotConfigured;
    };
    classify(within(persist.ping(), limit).await).0
}

pub(crate) async fn probe_latest_capture(
    persist: Option<&PersistHandle>,
    limit: Duration,
) -> (ProbeState, Option<chrono::DateTime<chrono::Utc>>) {
    let Some(persist) = persist else {
        return (ProbeState::NotConfigured, None);
    };
    let outcome = within(persist.latest_capture_write_at(), limit).await;
    if let Bounded::Done(Err(error)) = &outcome {
        tracing::warn!(%error, "health: failed to read latest capture write timestamp");
    }
    let (state, newest) = classify(outcome);
    (state, newest.flatten())
}

#[cfg(test)]
mod tests;
