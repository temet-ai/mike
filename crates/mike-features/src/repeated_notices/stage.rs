use bytes::Bytes;

use super::body::continues_a_thread;
use super::{NOTICES, Removed, Seen, memory, scan_body};
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::wire::manifest::AppliedStage;

const STAGE: &str = "repeated_notices";

pub struct RepeatedNoticesStage;

#[async_trait::async_trait]
impl RequestStage for RepeatedNoticesStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.repeated_notices && is_session_own_lane(ctx)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let remembered = memory::seen_by(ctx.session_id);
        let Some(scanned) = scan_body(body.body(), remembered) else {
            forget_when_the_history_holds_none(ctx.session_id, remembered, body.body());
            body.record_unchanged(STAGE, STAGE, "repeated_notices_none");
            return;
        };
        memory::remember(ctx.session_id, scanned.seen);
        let Some(thinned) = scanned.thinned else {
            body.record_unchanged(STAGE, STAGE, unchanged_reason(scanned.seen));
            return;
        };
        let removed_bytes = body.body().len().saturating_sub(thinned.len());
        tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            removed_bytes,
            removed = ?scanned.removed,
            "repeated notices removed from the forwarded body"
        );
        let new_bytes = Bytes::from(thinned);
        body.rewrite_forwarded_only(
            new_bytes.clone(),
            AppliedStage {
                stage: STAGE,
                policy_id: STAGE,
                policy_version: 3,
                resolved_parameters: parameters(&scanned.removed, removed_bytes),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "repeated_notices_removed",
                detail: None,
            },
        );
    }
}

fn unchanged_reason(seen: Seen) -> &'static str {
    if seen == Seen::default() {
        "repeated_notices_none"
    } else {
        "repeated_notices_first_kept"
    }
}

fn forget_when_the_history_holds_none(session_id: &str, remembered: Seen, raw: &[u8]) {
    if remembered != Seen::default() && !continues_a_thread(raw) {
        memory::remember(session_id, Seen::default());
    }
}

fn parameters(removed: &Removed, removed_bytes: usize) -> serde_json::Value {
    let by_notice: serde_json::Map<String, serde_json::Value> = NOTICES
        .iter()
        .zip(removed)
        .map(|(notice, count)| (notice.key.to_string(), serde_json::json!(count)))
        .collect();
    serde_json::json!({
        "removed": by_notice,
        "removed_bytes": removed_bytes,
    })
}

#[cfg(test)]
#[path = "stage_tests.rs"]
pub mod tests;
