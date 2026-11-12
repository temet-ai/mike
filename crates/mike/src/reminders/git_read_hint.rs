use std::sync::LazyLock;

use bytes::Bytes;
use dashmap::DashSet;
use serde_json::Value;

use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::state::AppState;
use crate::store::models::{ReminderSource, RequestId};
use crate::wire::manifest::AppliedStage;

use super::client_context::ContextBlock;
use super::seat_role::{SeatRole, seat_role};
use super::stage::{StageManifestCodes, record_outcome_without_body_change};
use super::{RemindersOutcome, try_append_reminder};

const MAX_TRACKED_SESSIONS: usize = 4096;
const READ_SUBCOMMANDS: [&str; 3] = ["status", "diff", "log"];

const HINT_TEXT: &str = "<system-reminder>\nTo see what other sessions changed in this checkout, call `what_changed` (view=changes) instead of git status/diff/log; view=mine lists your own uncommitted paths and whether the check ran after your last change.\n</system-reminder>";

const GIT_READ_HINT_CODES: StageManifestCodes = StageManifestCodes {
    stage: "git_read_hint",
    policy_id: "git_read_hint",
    delivered_out_of_band: "git_read_hint_delivered_out_of_band",
    nothing_to_inject: "git_read_hint_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

static SERVED_BY_CLIENT: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub(crate) async fn hand_over(state: &AppState, session_id: &str) -> Option<ContextBlock> {
    let toggles = state.feature_toggle_overlay.load();
    if !toggles.change_record || !toggles.client_serves_git_read_hint {
        return None;
    }
    if seat_role(state, session_id).await == SeatRole::Template {
        return None;
    }
    if SERVED_BY_CLIENT.len() >= MAX_TRACKED_SESSIONS {
        SERVED_BY_CLIENT.clear();
    }
    SERVED_BY_CLIENT.insert(session_id.to_owned());
    Some(ContextBlock {
        stage: "git_read_hint",
        text: HINT_TEXT.to_owned(),
    })
}

fn is_git_read_segment(segment: &str) -> bool {
    let mut tokens = segment.split_whitespace();
    if tokens.next() != Some("git") {
        return false;
    }
    while let Some(token) = tokens.next() {
        match token {
            "-C" | "-c" => {
                tokens.next();
            }
            t if t.starts_with('-') => {}
            sub => return READ_SUBCOMMANDS.contains(&sub),
        }
    }
    false
}

pub(super) fn is_git_read_command(command: &str) -> bool {
    command
        .split(['\n', ';', '|', '&'])
        .any(|segment| is_git_read_segment(segment.trim()))
}

pub(super) fn last_turn_read_git(body: &[u8]) -> Option<usize> {
    let parsed: Value = serde_json::from_slice(body).ok()?;
    let messages = parsed.get("messages")?.as_array()?;
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))?;
    let read_git = last_assistant
        .get("content")?
        .as_array()?
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter(|b| b.get("name").and_then(Value::as_str) == Some("Bash"))
        .filter_map(|b| b.pointer("/input/command").and_then(Value::as_str))
        .any(is_git_read_command);
    read_git.then_some(messages.len())
}

pub(super) fn hint_reminder(
    session_id: &str,
    body_bytes: &[u8],
    request_id: RequestId,
) -> RemindersOutcome {
    if last_turn_read_git(body_bytes).is_none()
        || !crate::explained::first_time(session_id, crate::explained::GIT_READ_HINT)
    {
        return RemindersOutcome::Unchanged;
    }
    try_append_reminder(
        body_bytes,
        ReminderSource::Diagnostic,
        HINT_TEXT.to_owned(),
        Vec::new(),
        0,
        request_id,
    )
}

pub(crate) struct GitReadHintStage;

#[async_trait::async_trait]
impl RequestStage for GitReadHintStage {
    fn name(&self) -> &'static str {
        "git_read_hint"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.state.feature_toggle_overlay.load().change_record && is_session_own_lane(ctx)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if SERVED_BY_CLIENT.contains(ctx.session_id) {
            body.record_unchanged(
                "git_read_hint",
                "git_read_hint",
                "git_read_hint_served_by_client",
            );
            return;
        }
        if seat_role(ctx.state, ctx.session_id).await == SeatRole::Template {
            body.record_unchanged(
                "git_read_hint",
                "git_read_hint",
                "git_read_hint_template_session",
            );
            return;
        }
        let outcome = hint_reminder(ctx.session_id, body.body(), ctx.request_id);
        let RemindersOutcome::Appended { body: new_body, .. } = outcome else {
            record_outcome_without_body_change(body, &GIT_READ_HINT_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "git_read_hint",
                policy_id: "git_read_hint",
                policy_version: 1,
                resolved_parameters: serde_json::json!({}),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "git_read_hint_injected",
                detail: Some(format!("injected {} bytes", HINT_TEXT.len())),
            },
        );
    }
}
