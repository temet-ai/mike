use bytes::Bytes;

use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::store::models::ThreadLink;
use crate::store::thread_links::FollowupAnswer;
use crate::wire::manifest::AppliedStage;

pub const STAGE: &str = "proxy_tool_thread";

pub struct ProxyToolThreadStage;

#[async_trait::async_trait]
impl RequestStage for ProxyToolThreadStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        true
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let Some(previous) = continued_from(body.body()) else {
            return;
        };
        match followup_answer(ctx, &previous).await {
            FollowupAnswer::EndsAt(end) => point_at(body, &previous, &end),
            FollowupAnswer::Unanswered => {
                tracing::warn!(
                    request_id = %ctx.request_id,
                    session_id = %ctx.session_id,
                    previous_message_id = %previous,
                    reason_code = "thread_followup_unanswered",
                    "thread continues from an answer whose proxy-tool follow-up never got a reply"
                );
                body.decline(
                    STAGE,
                    "thread_followup_unanswered",
                    serde_json::json!({ "previous_message_id": previous }),
                );
            }
            FollowupAnswer::NotFollowedUp | FollowupAnswer::NotHeld => {}
        }
    }
}

fn continued_from(body: &[u8]) -> Option<String> {
    let link = ThreadLink::from_body(body)?;
    if !link.continues() {
        return None;
    }
    link.previous_message_id
}

async fn followup_answer(ctx: &StageContext<'_>, previous: &str) -> FollowupAnswer {
    let held = ctx
        .state
        .store
        .read()
        .await
        .followup_answer(ctx.session_id, previous);
    if held != FollowupAnswer::NotHeld {
        return held;
    }
    let Some(persist) = ctx.state.persist() else {
        return held;
    };
    match persist.followup_answer(previous).await {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, "proxy_tool_thread: follow-up lookup failed");
            FollowupAnswer::NotHeld
        }
    }
}

fn point_at(body: &mut StageBody, previous: &str, end: &str) {
    let Some(new_body) = with_previous_message_id(body.body(), previous, end) else {
        return;
    };
    body.rewrite_forwarded_only(
        new_body.clone(),
        AppliedStage {
            stage: STAGE,
            policy_id: STAGE,
            policy_version: 1,
            resolved_parameters: serde_json::json!({
                "previous_message_id": previous,
                "followup_message_id": end,
            }),
            new_bytes: new_body.as_ref(),
            targets: Vec::new(),
            reason_code: "thread_continues_followup",
            detail: Some(format!("continue from {end} instead of {previous}")),
        },
    );
}

pub fn with_previous_message_id(body: &[u8], previous: &str, end: &str) -> Option<Bytes> {
    let text = std::str::from_utf8(body).ok()?;
    let thread = text.rfind("\"thread\"")?;
    let key = thread + text[thread..].find("\"previous_message_id\"")?;
    let quoted = format!("\"{previous}\"");
    let value = key + text[key..].find(&quoted)?;
    let mut out = String::with_capacity(text.len() + end.len());
    out.push_str(&text[..value]);
    out.push('"');
    out.push_str(end);
    out.push('"');
    out.push_str(&text[value + quoted.len()..]);
    let rewritten = Bytes::from(out);
    (continued_from(&rewritten).as_deref() == Some(end)).then_some(rewritten)
}

#[cfg(test)]
#[path = "thread_stage_tests.rs"]
mod tests;
