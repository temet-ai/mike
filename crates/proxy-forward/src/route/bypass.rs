use std::time::Instant;

use bytes::Bytes;
use http::{HeaderMap, Request, Response};

use super::decision::is_versioned_api_path;
use super::legs;
use super::{DispatchCtx, ResponseBody};

const SESSION_HEADER: &str = "x-claude-code-session-id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BypassReason {
    RoutedLegDown,
    RoutedLegFailedPreSend,
    NonApiPath,
}

impl BypassReason {
    fn label(self) -> &'static str {
        match self {
            Self::RoutedLegDown => "routed_leg_down",
            Self::RoutedLegFailedPreSend => "routed_leg_failed_pre_send",
            Self::NonApiPath => "non_api_path",
        }
    }

    fn skips_capture_of_an_api_call(self) -> bool {
        !matches!(self, Self::NonApiPath)
    }
}

pub(super) fn reason_for_direct_leg(path: &str) -> BypassReason {
    if is_versioned_api_path(path) {
        BypassReason::RoutedLegDown
    } else {
        BypassReason::NonApiPath
    }
}

pub(super) fn session_label(headers: &HeaderMap) -> &str {
    headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("-")
}

pub(super) async fn send_direct_logged(
    buffered: &Request<Bytes>,
    ctx: &DispatchCtx<'_>,
    reason: BypassReason,
) -> Response<ResponseBody> {
    let started = Instant::now();
    let response =
        legs::send_direct(buffered, ctx.clients, ctx.intercept_host, ctx.direct_target).await;
    let bypassed_since_up = if reason.skips_capture_of_an_api_call() {
        ctx.health.note_bypass()
    } else {
        0
    };
    log_sent_past_mike(
        buffered,
        reason,
        response.status().as_u16(),
        started.elapsed().as_millis() as u64,
        bypassed_since_up,
    );
    response
}

struct BypassLine<'a> {
    method: &'a str,
    path: &'a str,
    session_id: &'a str,
    reason: &'static str,
    body_bytes: usize,
    status: u16,
    elapsed_ms: u64,
}

fn log_sent_past_mike(
    buffered: &Request<Bytes>,
    reason: BypassReason,
    status: u16,
    elapsed_ms: u64,
    bypassed_since_up: u64,
) {
    let line = BypassLine {
        method: buffered.method().as_str(),
        path: buffered.uri().path(),
        session_id: session_label(buffered.headers()),
        reason: reason.label(),
        body_bytes: buffered.body().len(),
        status,
        elapsed_ms,
    };
    if reason.skips_capture_of_an_api_call() {
        warn_capture_lost(&line, bypassed_since_up);
    } else {
        info_not_captured(&line);
    }
}

fn warn_capture_lost(line: &BypassLine<'_>, bypassed_since_up: u64) {
    tracing::warn!(
        method = line.method,
        path = line.path,
        session_id = line.session_id,
        reason = line.reason,
        body_bytes = line.body_bytes,
        status = line.status,
        elapsed_ms = line.elapsed_ms,
        bypassed_since_up,
        "request sent past mike, NOT captured"
    );
}

fn info_not_captured(line: &BypassLine<'_>) {
    tracing::info!(
        method = line.method,
        path = line.path,
        session_id = line.session_id,
        reason = line.reason,
        body_bytes = line.body_bytes,
        status = line.status,
        elapsed_ms = line.elapsed_ms,
        "request sent past mike, NOT captured"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_api_path_on_direct_leg_is_a_down_bypass() {
        assert_eq!(
            reason_for_direct_leg("/v1/messages"),
            BypassReason::RoutedLegDown
        );
        assert_eq!(
            reason_for_direct_leg("/v2/messages"),
            BypassReason::RoutedLegDown
        );
    }

    #[test]
    fn non_api_path_on_direct_leg_is_not_a_capture_loss() {
        let reason = reason_for_direct_leg("/api/event_logging/v2/batch");
        assert_eq!(reason, BypassReason::NonApiPath);
        assert!(!reason.skips_capture_of_an_api_call());
    }

    #[test]
    fn down_and_pre_send_failure_both_skip_capture() {
        assert!(BypassReason::RoutedLegDown.skips_capture_of_an_api_call());
        assert!(BypassReason::RoutedLegFailedPreSend.skips_capture_of_an_api_call());
    }

    #[test]
    fn reason_labels_are_distinct() {
        let labels = [
            BypassReason::RoutedLegDown.label(),
            BypassReason::RoutedLegFailedPreSend.label(),
            BypassReason::NonApiPath.label(),
        ];
        assert_eq!(labels[0], "routed_leg_down");
        assert_ne!(labels[0], labels[1]);
        assert_ne!(labels[1], labels[2]);
    }

    #[test]
    fn session_label_reads_the_claude_code_header() -> Result<(), http::header::InvalidHeaderValue>
    {
        let mut headers = HeaderMap::new();
        headers.insert(
            SESSION_HEADER,
            http::HeaderValue::from_str(" 05e68e12-74f8-424f-8afb-8ffa65f6289a ")?,
        );
        assert_eq!(
            session_label(&headers),
            "05e68e12-74f8-424f-8afb-8ffa65f6289a"
        );
        Ok(())
    }

    #[test]
    fn session_label_without_header_is_a_dash() {
        assert_eq!(session_label(&HeaderMap::new()), "-");
    }
}
