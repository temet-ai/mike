use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::*;

pub(crate) type FindTextFailure = (StatusCode, Json<FindTextErrorBody>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanLimits {
    pub(crate) body_window_requests: usize,
    pub(crate) message_window_requests: usize,
    pub(crate) budget: Duration,
}

impl ScanLimits {
    pub(crate) fn production() -> Self {
        Self {
            body_window_requests: FIND_TEXT_SESSION_PAGE_REQUESTS,
            message_window_requests: FIND_TEXT_SESSION_MESSAGE_WINDOW_REQUESTS,
            budget: Duration::from_secs(FIND_TEXT_SESSION_BUDGET_SECS),
        }
    }

    fn window_requests(&self, params: &FindTextMatchParams<'_>) -> usize {
        let window = if params.reads_messages() {
            self.message_window_requests
        } else {
            self.body_window_requests
        };
        window.max(1)
    }
}

pub(super) struct ScanCtx<'a> {
    pub(super) state: &'a AppState,
    pub(super) handle: &'a crate::store::persistence::PersistHandle,
    pub(super) session_db_id: i64,
    pub(super) session_id: &'a str,
    pub(super) params: &'a FindTextMatchParams<'a>,
}

#[derive(Default)]
pub(super) struct ScanAcc {
    pub(super) matches: Vec<UnifiedTextMatch>,
    pub(super) full_count: u64,
    pub(super) reminder_skipped: u64,
    pub(super) own_skipped: u64,
    pub(super) own_calls: OwnSearchCalls,
}

impl ScanAcc {
    pub(super) fn has_room(&self, cap: u32) -> bool {
        self.matches.len() < cap as usize
    }
}

pub(super) struct SessionScan {
    pub(super) acc: ScanAcc,
    pub(super) scanned: usize,
    pub(super) last_scanned: Option<u64>,
    pub(super) complete: bool,
}

pub(super) fn unified_match(
    ctx: &ScanCtx<'_>,
    request_id: u64,
    message_ord: Option<i32>,
    m: crate::api::body_search::TextMatch,
) -> UnifiedTextMatch {
    UnifiedTextMatch {
        request_id,
        message_ord,
        session_id: ctx.session_id.to_owned(),
        source: ctx.params.source.as_wire_str(),
        byte_offset: m.byte_offset,
        match_length: m.match_length,
        line: m.line,
        column: m.column,
        before: m.before,
        matched: m.matched,
        after: m.after,
    }
}

pub(super) async fn scan_session(
    ctx: &ScanCtx<'_>,
    request_ids: &[i64],
    limits: &ScanLimits,
) -> Result<SessionScan, FindTextFailure> {
    let started = Instant::now();
    let mut acc = ScanAcc::default();
    let mut scanned = 0usize;
    let mut last_scanned = None;
    for window in request_ids.chunks(limits.window_requests(ctx.params)) {
        scan_window(ctx, window, &mut acc).await?;
        scanned += window.len();
        last_scanned = window.last().map(|id| *id as u64);
        if scanned < request_ids.len() && started.elapsed() >= limits.budget {
            break;
        }
    }
    Ok(SessionScan {
        acc,
        scanned,
        last_scanned,
        complete: scanned == request_ids.len(),
    })
}

async fn scan_window(
    ctx: &ScanCtx<'_>,
    window: &[i64],
    acc: &mut ScanAcc,
) -> Result<(), FindTextFailure> {
    if ctx.params.reads_messages() {
        scan_message_window(ctx, window, acc).await
    } else {
        scan_body_window(ctx, window, acc).await
    }
}

async fn fetch_scan_batch_bodies(
    ctx: &ScanCtx<'_>,
    page_ids: &[i64],
) -> Result<HashMap<i64, Bytes>, FindTextFailure> {
    let fetched = match ctx.params.source {
        FindTextSource::Response => ctx.handle.fetch_response_bodies_batch(page_ids).await,
        FindTextSource::Request => ctx.handle.fetch_request_bodies_batch(page_ids).await,
        FindTextSource::RequestSent => Ok(HashMap::new()),
    };
    fetched.map_err(|e| {
        tracing::warn!(error = %e, session_id = %ctx.session_id, "find_text: batched body fetch failed");
        find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
    })
}

async fn body_for_request(
    ctx: &ScanCtx<'_>,
    request_id: i64,
    batched: &HashMap<i64, Bytes>,
) -> Option<Bytes> {
    if ctx.params.source == FindTextSource::RequestSent {
        return resolve_body_for_find(ctx.state, request_id as u64, ctx.params.source)
            .await
            .ok()
            .flatten()
            .flatten();
    }
    batched.get(&request_id).cloned()
}

async fn scan_body_window(
    ctx: &ScanCtx<'_>,
    window: &[i64],
    acc: &mut ScanAcc,
) -> Result<(), FindTextFailure> {
    let params = ctx.params;
    let batched = fetch_scan_batch_bodies(ctx, window).await?;
    let own_requests = if params.source == FindTextSource::Response {
        own_search_request_ids(ctx.handle, window, params.needle, params.case_sensitive).await?
    } else {
        HashSet::new()
    };
    for &request_id in window {
        let Some(body) = body_for_request(ctx, request_id, &batched).await else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&body) else {
            continue;
        };
        let room = params.max_matches.saturating_sub(acc.matches.len() as u32);
        let (count, found) = crate::api::body_search::find_in_str(
            text,
            params.needle,
            params.case_sensitive,
            params.context_bytes,
            room,
        );
        if own_requests.contains(&request_id) {
            acc.own_skipped += count;
            continue;
        }
        acc.full_count += count;
        for m in found {
            acc.matches
                .push(unified_match(ctx, request_id as u64, None, m));
        }
    }
    Ok(())
}
