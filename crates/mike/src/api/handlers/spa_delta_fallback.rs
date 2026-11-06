use axum::Json;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{MethodRouter, post};
use serde::Deserialize;

const COUNTER_SPA_DELTA_FALLBACK_TOTAL: &str = "proxy_spa_delta_fallback_total";
const MAX_BEACON_BYTES: usize = 256;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SpaDeltaFallbackReason {
    PredecessorNotLoaded,
    Compaction,
    LineageBroken,
    ChromeStripFailed,
    ParseError,
}

impl SpaDeltaFallbackReason {
    fn label(self) -> &'static str {
        match self {
            Self::PredecessorNotLoaded => "predecessor-not-loaded",
            Self::Compaction => "compaction",
            Self::LineageBroken => "lineage-broken",
            Self::ChromeStripFailed => "chrome-strip-failed",
            Self::ParseError => "parse-error",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpaDeltaFallback {
    reason: SpaDeltaFallbackReason,
}

pub(crate) fn spa_delta_fallback_route<S>() -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    metrics::describe_counter!(
        COUNTER_SPA_DELTA_FALLBACK_TOTAL,
        "SPA turn-delta fallbacks grouped by a fixed reason vocabulary"
    );
    post(record_spa_delta_fallback).layer(DefaultBodyLimit::max(MAX_BEACON_BYTES))
}

async fn record_spa_delta_fallback(Json(beacon): Json<SpaDeltaFallback>) -> StatusCode {
    metrics::counter!(COUNTER_SPA_DELTA_FALLBACK_TOTAL, "reason" => beacon.reason.label())
        .increment(1);
    StatusCode::NO_CONTENT
}

#[cfg(test)]
#[path = "spa_delta_fallback_tests.rs"]
mod tests;
