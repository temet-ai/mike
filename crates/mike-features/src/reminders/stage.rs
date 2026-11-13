use bytes::Bytes;

use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::reminders::{self, RemindersConfig, RemindersOutcome};
use crate::wire::manifest::AppliedStage;

pub struct StageManifestCodes {
    pub stage: &'static str,
    pub policy_id: &'static str,
    pub delivered_out_of_band: &'static str,
    pub nothing_to_inject: &'static str,
    pub source_finding_ids_are_lesson_ids: bool,
}

const SELF_IDENTITY_CODES: StageManifestCodes = StageManifestCodes {
    stage: "self_identity",
    policy_id: "self_identity",
    delivered_out_of_band: "self_identity_delivered_out_of_band",
    nothing_to_inject: "self_identity_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

const DIAGNOSTIC_REMINDERS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "reminders",
    policy_id: "reminders",
    delivered_out_of_band: "diagnostic_reminder_delivered_out_of_band",
    nothing_to_inject: "diagnostic_reminder_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

pub fn record_outcome_without_body_change(
    body: &mut StageBody,
    codes: &StageManifestCodes,
    outcome: &RemindersOutcome,
) {
    match outcome {
        RemindersOutcome::Appended { .. } => {}
        RemindersOutcome::DeliveredOutOfBand {
            reminder_text,
            source_finding_ids,
            source_request_id,
        } => {
            tracing::info!(
                stage = codes.stage,
                reminder_text_len = reminder_text.len(),
                source_request_id,
                source_finding_ids = ?source_finding_ids,
                "reminder delivered out of band — request bytes unchanged"
            );
            if codes.source_finding_ids_are_lesson_ids {
                body.merge_lesson_ids(source_finding_ids.iter().copied());
            }
            body.record_unchanged(codes.stage, codes.policy_id, codes.delivered_out_of_band);
        }
        RemindersOutcome::Suppressed { reason } => {
            body.record_unchanged(
                codes.stage,
                codes.policy_id,
                &format!("{}_suppressed_{reason}", codes.policy_id),
            );
        }
        RemindersOutcome::Unchanged => {
            body.record_unchanged(codes.stage, codes.policy_id, codes.nothing_to_inject);
        }
    }
}

pub struct SelfIdentityStage;

#[async_trait::async_trait]
impl RequestStage for SelfIdentityStage {
    fn name(&self) -> &'static str {
        "self_identity"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.self_identity_reminder && is_session_own_lane(ctx) && !ctx.is_tool_loop
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if ctx.state.self_identity_served.contains(ctx.session_id)
            && super::client_context::client_text_reached(
                body.body(),
                &format!(
                    "{}{}",
                    crate::store::models::SELF_IDENTITY_ID_LINE,
                    ctx.session_id
                ),
            )
        {
            body.record_unchanged(
                "self_identity",
                "self_identity",
                "self_identity_served_by_client",
            );
            return;
        }
        let role = super::seat_role::seat_role(ctx.state, ctx.session_id).await;
        if role == super::seat_role::SeatRole::Template {
            body.record_unchanged(
                "self_identity",
                "self_identity",
                "self_identity_template_session",
            );
            return;
        }
        let push_deps = ctx.state.push_deps();
        let outcome = reminders::apply_self_identity(reminders::SelfIdentityRequest {
            body_bytes: body.body(),
            session_id: ctx.session_id,
            request_id: ctx.request_id,
            push_deps: &push_deps,
        })
        .await;
        let outcome = super::fork_prefix::keep_inherited_prefix(role, body.body(), outcome);
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            ..
        } = outcome
        else {
            record_outcome_without_body_change(body, &SELF_IDENTITY_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            reminder_len = reminder_text.len(),
            "self-identity injected <system-reminder> block"
        );
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "self_identity",
                policy_id: "self_identity",
                policy_version: 1,
                resolved_parameters: serde_json::json!({}),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "self_identity_injected",
                detail: Some(format!("injected {} bytes", reminder_text.len())),
            },
        );
    }
}

pub struct RemindersStage;

#[async_trait::async_trait]
impl RequestStage for RemindersStage {
    fn name(&self) -> &'static str {
        "reminders"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.reminders && is_session_own_lane(ctx)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if super::client_context::reminders_served_by_client(ctx.session_id) {
            body.record_unchanged("reminders", "reminders", "reminders_served_by_client");
            return;
        }
        let cfg = RemindersConfig {
            enabled: ctx.session_toggles.reminders,
            inject_lessons: ctx.session_toggles.reminders_inject_lessons,
            max_lessons_to_inject: ctx.session_toggles.reminders_max_lessons_to_inject,
        };
        let push_deps = ctx.state.push_deps();
        let outcome = reminders::apply_reminders(
            body.body(),
            ctx.session_id,
            &ctx.state.store,
            &cfg,
            ctx.request_id,
            &push_deps,
        )
        .await;
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            source_finding_ids,
            source_request_id,
        } = outcome
        else {
            record_outcome_without_body_change(body, &DIAGNOSTIC_REMINDERS_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        tracing::info!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            source_request_id = source_request_id,
            finding_count = source_finding_ids.len(),
            reminder_text_len = reminder_text.len(),
            "reminders appended <system-reminder> block"
        );
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "reminders",
                policy_id: "reminders",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "source_request_id": source_request_id,
                    "source_finding_ids": source_finding_ids,
                }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "diagnostic_reminder_injected",
                detail: Some(format!(
                    "{} finding(s) from request {source_request_id}",
                    source_finding_ids.len()
                )),
            },
        );
    }
}
