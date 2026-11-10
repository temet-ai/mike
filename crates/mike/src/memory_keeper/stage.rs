use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;

use super::block::{Block, KEEPER_MARKER_OPEN};
use super::earlier::{EARLIER_MARKER_OPEN, operator_words};
use super::earlier_lookup::{EarlierAsk, earlier_work};
use super::record::spawn_record;
use super::served::served_by_client;
use super::supply::{Carrier, SupplyRequest, supply};
use crate::compress::manifest::AppliedStage;
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::reminders::appender::append_block_to_last_user_message;
use crate::reminders::session_lessons_query::recent_user_prompts;

pub(crate) const STAGE: &str = "memory_keeper";
const RECENT_PROMPTS: usize = 4;
const EARLIER_BUDGET: Duration = Duration::from_millis(900);

pub(crate) struct MemoryKeeperStage;

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
        let Ok(session) = Uuid::parse_str(ctx.session_id) else {
            return;
        };
        let (prompts, _) = recent_user_prompts(body.body(), RECENT_PROMPTS);
        let card_served =
            served_by_client(ctx.session_id) || carries(body.body(), KEEPER_MARKER_OPEN);
        let card = if card_served {
            None
        } else {
            card(ctx, session, &prompts)
        };
        let earlier = earlier(ctx, session, prompts.first()).await;
        let blocks: Vec<Block> = card.into_iter().chain(earlier).collect();
        if blocks.is_empty() {
            let reason = if card_served {
                "memory_keeper_served_by_client"
            } else {
                "memory_keeper_nothing_to_supply"
            };
            body.record_unchanged(STAGE, STAGE, reason);
            return;
        }
        supply_blocks(ctx, body, &blocks);
    }
}

fn supply_blocks(ctx: &StageContext<'_>, body: &mut StageBody, blocks: &[Block]) {
    let text = blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let Some(new_body) = append_block_to_last_user_message(body.body(), &text, ctx.request_id)
    else {
        body.record_unchanged(STAGE, STAGE, "memory_keeper_no_user_turn");
        return;
    };
    let hashes: Vec<&str> = blocks.iter().map(|block| block.hash.as_str()).collect();
    let new_bytes = Bytes::from(new_body);
    body.rewrite(
        new_bytes.clone(),
        AppliedStage {
            stage: STAGE,
            policy_id: STAGE,
            policy_version: 1,
            resolved_parameters: serde_json::json!({ "block_hashes": hashes }),
            new_bytes: new_bytes.as_ref(),
            targets: Vec::new(),
            reason_code: "memory_keeper_card_supplied",
            detail: Some(format!(
                "supplied {} bytes of memory-keeper context",
                text.len()
            )),
        },
    );
}

fn carries(body: &[u8], marker: &str) -> bool {
    let marker = marker.as_bytes();
    body.windows(marker.len()).any(|window| window == marker)
}

fn card(ctx: &StageContext<'_>, session: Uuid, prompts: &[String]) -> Option<Block> {
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

async fn earlier(ctx: &StageContext<'_>, session: Uuid, last: Option<&String>) -> Option<Block> {
    let last = last?;
    if last.contains(EARLIER_MARKER_OPEN) {
        return None;
    }
    let prompt = operator_words(last);
    let ask = EarlierAsk {
        session,
        project_path: ctx.project_path,
        prompt: &prompt,
        carrier: Carrier::Proxy,
    };
    let supply = tokio::time::timeout(EARLIER_BUDGET, earlier_work(ctx.state, &ask))
        .await
        .ok()
        .flatten()?;
    spawn_record(ctx.state.persist().cloned(), supply.record);
    supply.block
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
