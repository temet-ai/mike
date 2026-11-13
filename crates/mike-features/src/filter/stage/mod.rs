use crate::store::persistence::proxy_repo::ProxyPersist as _;
#[cfg(test)]
pub mod decline_tests;
#[cfg(test)]
pub mod tests;

use bytes::Bytes;

use crate::extract::request::TurnOrigin;
use crate::filter::replay::{SystemReplay, ruleset_fingerprint};
use crate::filter::{self, FilterOutcome, project_scope_matches};
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::wire::manifest::AppliedStage;

pub struct FilterStage;

impl FilterStage {
    fn apply_fresh(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let original = body.body().clone();
        let outcome = filter::apply_request_filter(&original, ctx.filter_cfg, ctx.project_path);
        let FilterOutcome::Rewrote {
            new_body,
            original_len,
            filtered_len,
            firing_rule_ids,
        } = outcome
        else {
            ctx.state.system_replay.mark_unfiltered(ctx.session_id);
            body.decline(
                "filter",
                fresh_decline_reason(ctx),
                serde_json::json!({ "rule_count": ctx.filter_cfg.rules.len() }),
            );
            return;
        };

        log_fresh_rewrite(ctx, original_len, filtered_len, &firing_rule_ids);
        capture_replay(ctx, &original, &new_body, &firing_rule_ids);

        body.merge_rule_ids(firing_rule_ids.iter().copied());
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: "filter",
                policy_id: "filter",
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "firing_rule_ids": firing_rule_ids }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "filter_rule_matched",
                detail: Some(format!("rules fired: {firing_rule_ids:?}")),
            },
        );
    }

    fn apply_replay(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let Some(replay) = ctx.state.system_replay.get(ctx.session_id) else {
            body.decline("filter", "filter_replay_missing", serde_json::json!({}));
            return;
        };
        let ruleset_changed =
            !replay.matches_ruleset(ruleset_fingerprint(ctx.filter_cfg, ctx.project_path));
        if ruleset_changed {
            log_pinned_stale_replay(ctx);
        }
        let Some(new_body) = replay.apply(body.body()) else {
            log_replay_skipped(ctx);
            body.decline(
                "filter",
                "filter_replay_system_mismatch",
                serde_json::json!({ "ruleset_changed": ruleset_changed }),
            );
            return;
        };
        let firing_rule_ids = replay.firing_rule_ids();
        log_replay_applied(ctx, firing_rule_ids);
        body.merge_rule_ids(firing_rule_ids.iter().copied());
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: "filter",
                policy_id: "filter_replay",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "firing_rule_ids": firing_rule_ids,
                    "ruleset_changed": ruleset_changed,
                }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "filter_replay_applied",
                detail: Some(format!("rules replayed: {firing_rule_ids:?}")),
            },
        );
    }
}

impl FilterStage {
    async fn recover(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if !was_filtered_before(ctx).await {
            ctx.state.system_replay.mark_unfiltered(ctx.session_id);
            body.decline("filter", "filter_not_in_force", serde_json::json!({}));
            return;
        }
        let original = body.body().clone();
        if let FilterOutcome::Rewrote {
            new_body,
            firing_rule_ids,
            ..
        } = filter::apply_request_filter(&original, ctx.filter_cfg, ctx.project_path)
        {
            capture_replay(ctx, &original, &new_body, &firing_rule_ids);
        }
        if ctx.state.system_replay.has(ctx.session_id) {
            tracing::info!(
                request_id = %ctx.request_id,
                session_id = %ctx.session_id,
                "filter: rebuilt this session's system rewrite after a restart"
            );
            self.apply_replay(ctx, body);
        } else {
            ctx.state.system_replay.mark_unfiltered(ctx.session_id);
            body.decline("filter", "filter_recovery_no_match", serde_json::json!({}));
        }
    }
}

async fn was_filtered_before(ctx: &StageContext<'_>) -> bool {
    let (Some(persist), Ok(session)) = (ctx.state.persist(), ctx.session_id.parse::<uuid::Uuid>())
    else {
        return false;
    };
    match persist
        .proxy()
        .session_system_prompt_is_filtered(session)
        .await
    {
        Ok(filtered) => filtered,
        Err(err) => {
            tracing::warn!(session_id = %ctx.session_id, error = %err, "filter: could not read whether this session's system prompt was rewritten before the restart");
            false
        }
    }
}

fn fresh_decline_reason(ctx: &StageContext<'_>) -> &'static str {
    if ctx.filter_cfg.rules.is_empty() {
        return "filter_no_rules";
    }
    let any_applicable = ctx.filter_cfg.rules.iter().any(|r| {
        r.enabled
            && r.project_path
                .as_deref()
                .is_none_or(|p| project_scope_matches(p, ctx.project_path))
    });
    if any_applicable {
        "filter_no_match"
    } else {
        "filter_no_applicable_rules"
    }
}

fn log_fresh_rewrite(
    ctx: &StageContext<'_>,
    original_len: usize,
    filtered_len: usize,
    firing_rule_ids: &[i64],
) {
    tracing::info!(
        request_id = %ctx.request_id,
        original_bytes = original_len,
        filtered_bytes = filtered_len,
        delta = original_len.saturating_sub(filtered_len),
        rule_ids = ?firing_rule_ids,
        "request filter rewrote body"
    );
}

fn capture_replay(
    ctx: &StageContext<'_>,
    original: &Bytes,
    new_body: &Bytes,
    firing_rule_ids: &[i64],
) {
    let fingerprint = ruleset_fingerprint(ctx.filter_cfg, ctx.project_path);
    match SystemReplay::capture(original, new_body, firing_rule_ids, fingerprint) {
        Some(replay) => ctx.state.system_replay.insert(ctx.session_id, replay),
        None => tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            "filter rewrote the body but produced no replayable system[] region"
        ),
    }
}

fn log_pinned_stale_replay(ctx: &StageContext<'_>) {
    tracing::info!(
        request_id = %ctx.request_id,
        session_id = %ctx.session_id,
        "filter rule set changed since this session's rewrite was captured; replaying the captured rewrite to keep the prompt cache intact"
    );
}

fn log_replay_skipped(ctx: &StageContext<'_>) {
    tracing::debug!(
        request_id = %ctx.request_id,
        session_id = %ctx.session_id,
        "filter replay skipped — this request's system[] is not the captured one"
    );
}

fn log_replay_applied(ctx: &StageContext<'_>, firing_rule_ids: &[i64]) {
    tracing::debug!(
        request_id = %ctx.request_id,
        rule_ids = ?firing_rule_ids,
        "filter replay spliced the session's system rewrite"
    );
}

#[async_trait::async_trait]
impl RequestStage for FilterStage {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        match ctx.turn_origin {
            TurnOrigin::PrimaryFirst => true,
            TurnOrigin::Primary => {
                ctx.state.system_replay.has(ctx.session_id)
                    || !ctx.state.system_replay.is_unfiltered(ctx.session_id)
            }
            _ => ctx.state.system_replay.has(ctx.session_id),
        }
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if matches!(ctx.turn_origin, TurnOrigin::PrimaryFirst) {
            self.apply_fresh(ctx, body);
        } else if ctx.state.system_replay.has(ctx.session_id) {
            self.apply_replay(ctx, body);
        } else {
            self.recover(ctx, body).await;
        }
    }
}
