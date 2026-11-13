use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;

use super::block::{Block, KEEPER_MARKER_OPEN};
use super::record::spawn_record;
use super::served::served_by_client;
use super::supply::{Carrier, SupplyRequest, supply};
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::reminders::appender::append_block_to_last_user_message;
use crate::reminders::session_lessons_query::recent_user_prompts;
use crate::wire::manifest::AppliedStage;

pub const STAGE: &str = "memory_keeper";
const RECENT_PROMPTS: usize = 4;

pub struct MemoryKeeperStage;

#[async_trait::async_trait]
impl RequestStage for MemoryKeeperStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.state.feature_toggle_overlay.load().memory_keeper
            && is_session_own_lane(ctx)
            && !ctx.is_tool_loop
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if served_by_client(ctx.session_id) || carries_marker(body.body()) {
            body.record_unchanged(STAGE, STAGE, "memory_keeper_served_by_client");
            return;
        }
        let Ok(session) = Uuid::parse_str(ctx.session_id) else {
            return;
        };
        let Some(block) = card(ctx, session, body.body()) else {
            body.record_unchanged(STAGE, STAGE, "memory_keeper_nothing_to_supply");
            return;
        };
        let Some(new_body) =
            append_block_to_last_user_message(body.body(), &block.text, ctx.request_id)
        else {
            body.record_unchanged(STAGE, STAGE, "memory_keeper_no_user_turn");
            return;
        };
        let new_bytes = Bytes::from(new_body);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: STAGE,
                policy_id: STAGE,
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "card_hash": block.hash }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "memory_keeper_card_supplied",
                detail: Some(format!(
                    "supplied {} bytes of thread card",
                    block.text.len()
                )),
            },
        );
    }
}

fn carries_marker(body: &[u8]) -> bool {
    let marker = KEEPER_MARKER_OPEN.as_bytes();
    body.windows(marker.len()).any(|window| window == marker)
}

fn card(ctx: &StageContext<'_>, session: Uuid, body: &[u8]) -> Option<Block> {
    let (prompts, _) = recent_user_prompts(body, RECENT_PROMPTS);
    let (prompt, recent) = prompts.split_first()?;
    let snapshot = ctx.state.keeper_snapshot.load();
    let outcome = supply(
        &snapshot,
        &SupplyRequest {
            session,
            project_path: ctx.project_path,
            prompt: Some(prompt),
            cwd: None,
            branch: None,
            recent: recent.iter().map(String::as_str).collect(),
            carrier: Carrier::Proxy,
            now: Utc::now(),
        },
    );
    if let Some(record) = outcome.record {
        spawn_record(ctx.state.persist().cloned(), record);
    }
    outcome.block
}

#[cfg(test)]
#[path = "stage_tests.rs"]
pub mod tests;
