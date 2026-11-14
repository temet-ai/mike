use bytes::Bytes;
use uuid::Uuid;

use crate::launch::session_tail::session_tail_lines;
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::store::models::{ReminderSource, RequestId};
use crate::store::persistence::PersistHandle;
use crate::wire::manifest::AppliedStage;

use super::seat_role::{SeatRole, seat_role};
use super::stage::{StageManifestCodes, record_outcome_without_body_change};
use super::{RemindersOutcome, try_append_reminder};

pub const TAIL_HEADING: &str = "What others changed since you last looked:";

const CHANGE_RECORD_TAIL_CODES: StageManifestCodes = StageManifestCodes {
    stage: "change_record_tail",
    policy_id: "change_record_tail",
    delivered_out_of_band: "change_record_tail_delivered_out_of_band",
    nothing_to_inject: "change_record_tail_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

pub fn tail_reminder_text(lines: &[String]) -> String {
    let mut text = format!("<system-reminder>\n{TAIL_HEADING}\n");
    for line in lines {
        text.push_str("- ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("</system-reminder>");
    text
}

pub async fn tail_reminder(
    persist: &PersistHandle,
    session: Uuid,
    body_bytes: &[u8],
    request_id: RequestId,
) -> RemindersOutcome {
    let Some(lines) = session_tail_lines(persist, session).await else {
        return RemindersOutcome::Unchanged;
    };
    try_append_reminder(
        body_bytes,
        ReminderSource::Diagnostic,
        tail_reminder_text(&lines),
        Vec::new(),
        0,
        request_id,
    )
}

pub struct ChangeRecordTailStage;

#[async_trait::async_trait]
impl RequestStage for ChangeRecordTailStage {
    fn name(&self) -> &'static str {
        "change_record_tail"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.state.feature_toggle_overlay.load().change_record_tail
            && is_session_own_lane(ctx)
            && !ctx.is_tool_loop
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if ctx.state.change_record_tail_served.contains(ctx.session_id) {
            body.record_unchanged(
                "change_record_tail",
                "change_record_tail",
                "change_record_tail_served_by_client",
            );
            return;
        }
        let (Some(persist), Ok(session)) = (ctx.state.persist(), ctx.session_id.parse::<Uuid>())
        else {
            record_outcome_without_body_change(
                body,
                &CHANGE_RECORD_TAIL_CODES,
                &RemindersOutcome::Unchanged,
            );
            return;
        };
        if seat_role(ctx.state, ctx.session_id).await == SeatRole::Template {
            body.record_unchanged(
                "change_record_tail",
                "change_record_tail",
                "change_record_tail_template_session",
            );
            return;
        }
        let outcome = tail_reminder(persist, session, body.body(), ctx.request_id).await;
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            ..
        } = outcome
        else {
            record_outcome_without_body_change(body, &CHANGE_RECORD_TAIL_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "change_record_tail",
                policy_id: "change_record_tail",
                policy_version: 1,
                resolved_parameters: serde_json::json!({}),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "change_record_tail_injected",
                detail: Some(format!("injected {} bytes", reminder_text.len())),
            },
        );
    }
}
