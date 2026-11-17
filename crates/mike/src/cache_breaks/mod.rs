pub(crate) mod attribute;
pub(crate) mod detect;
pub(crate) mod health;
pub(crate) mod load;
pub(crate) mod normalize;
pub(crate) mod reads;
pub(crate) mod store;

use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use detect::{Candidate, Kind, Verdict};
use store::Found;

const TICK: Duration = Duration::from_secs(60);
const BATCH_IDS: i64 = 2_000;
const BACKFILL_DAYS: i64 = 7;

#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub(crate) struct ScanOutcome {
    pub(crate) scanned_to: i64,
    pub(crate) recorded: u64,
}

struct Named {
    cause: &'static str,
    stage: Option<String>,
    location: Option<&'static str>,
    source: Option<i64>,
    detail: serde_json::Value,
}

impl Named {
    fn plain(
        cause: &'static str,
        location: Option<&'static str>,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            cause,
            stage: None,
            location,
            source: None,
            detail,
        }
    }
}

fn found(c: &Candidate, v: &Verdict, n: Named) -> Found {
    let Named {
        cause,
        stage,
        location,
        source,
        detail,
    } = n;
    Found {
        request_id: c.request_id,
        session_id: c.session_id,
        prev_request_id: c.prev_request_id,
        source_request_id: source,
        kind: v.kind.as_str(),
        cause,
        stage,
        location,
        woken_by: Some(c.woken_by.clone()),
        ttl_seconds: v.ttl_seconds,
        gap_seconds: v.gap_seconds,
        expected_read: v.expected_read,
        cache_read: v.cache_read,
        tokens_rewritten: v.tokens_rewritten,
        detail,
        occurred_at: c.created_at,
    }
}

async fn attributed(persist: &PersistHandle, c: &Candidate, v: &Verdict) -> Found {
    match load::load(persist, c.session_id, c.request_id, c.prev_request_id).await {
        Ok((cur, chain)) => {
            let a = attribute::attribute(&cur, &chain);
            if a.cause == attribute::Cause::Mike {
                tracing::warn!(
                    request_id = c.request_id,
                    session_id = c.session_id,
                    stage = a.stage.as_deref().unwrap_or_default(),
                    location = a.location,
                    tokens_rewritten = v.tokens_rewritten,
                    "prompt cache break caused by a mike stage"
                );
            }
            let named = Named {
                cause: a.cause.as_str(),
                stage: a.stage,
                location: Some(a.location),
                source: a.source_request_id,
                detail: a.detail,
            };
            found(c, v, named)
        }
        Err(e) => {
            let detail = json!({ "error": e.to_string() });
            found(c, v, Named::plain("unknown", Some("unreadable"), detail))
        }
    }
}

async fn judge_batch(persist: &PersistHandle, candidates: &[Candidate]) -> Vec<Found> {
    let mut out = Vec::new();
    for c in candidates {
        let Some(v) = detect::judge(c) else { continue };
        let row = match v.kind {
            Kind::Expiry => found(c, &v, Named::plain("expired", None, json!({}))),
            Kind::Break => attributed(persist, c, &v).await,
        };
        out.push(row);
    }
    out
}

async fn start_cursor(persist: &PersistHandle) -> Result<i64, sqlx::Error> {
    let pool = persist.pool();
    if let Some(at) = store::cursor(pool).await? {
        return Ok(at);
    }
    let since = Utc::now() - chrono::Duration::days(BACKFILL_DAYS);
    store::first_response_since(pool, since).await
}

pub(crate) async fn scan(
    persist: &PersistHandle,
    shutdown: &CancellationToken,
) -> Result<ScanOutcome, sqlx::Error> {
    let pool = persist.pool();
    let mut at = start_cursor(persist).await?;
    let upper = store::settled_upper(pool).await?;
    let mut recorded = 0;
    while at < upper && !shutdown.is_cancelled() {
        let to = (at + BATCH_IDS).min(upper);
        let candidates = store::candidates(pool, at, to).await?;
        let rows = judge_batch(persist, &candidates).await;
        recorded += store::insert(pool, &rows).await?;
        store::set_cursor(pool, to).await?;
        at = to;
    }
    Ok(ScanOutcome {
        scanned_to: at,
        recorded,
    })
}

pub(crate) async fn rescan_from(persist: &PersistHandle, days: i64) -> Result<(), sqlx::Error> {
    let since = Utc::now() - chrono::Duration::days(days);
    let first = store::first_response_since(persist.pool(), since).await?;
    store::set_cursor(persist.pool(), first).await
}

fn log_scan(result: &Result<ScanOutcome, sqlx::Error>) {
    match result {
        Ok(o) if o.recorded > 0 => log_recorded(o),
        Ok(_) => {}
        Err(e) => log_scan_failed(e),
    }
}

fn log_recorded(outcome: &ScanOutcome) {
    tracing::info!(
        recorded = outcome.recorded,
        scanned_to = outcome.scanned_to,
        "prompt cache scan recorded breaks"
    );
}

fn log_scan_failed(error: &sqlx::Error) {
    tracing::warn!(%error, "prompt cache scan failed");
}

async fn run(persist: PersistHandle, shutdown: CancellationToken) {
    loop {
        log_scan(&scan(&persist, &shutdown).await);
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(TICK) => {}
        }
    }
}

pub fn spawn_cache_break_scanner(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        return tracing::debug!("prompt cache scan: not spawning without persistence");
    };
    state.tracker.spawn(run(persist, state.shutdown()));
    tracing::info!(tick_secs = TICK.as_secs(), "prompt cache scan: spawned");
}
