use bytes::Bytes;

use crate::compress::manifest::AppliedStage;
use crate::extract::request::TurnOrigin;
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::reminders::{self, RemindersConfig, RemindersOutcome, SessionLessonsRequest};

pub(super) struct StageManifestCodes {
    pub(super) stage: &'static str,
    pub(super) policy_id: &'static str,
    pub(super) delivered_out_of_band: &'static str,
    pub(super) nothing_to_inject: &'static str,
    pub(super) source_finding_ids_are_lesson_ids: bool,
}

const SESSION_LESSONS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "session_lessons",
    policy_id: "session_lessons",
    delivered_out_of_band: "session_lessons_delivered_out_of_band",
    nothing_to_inject: "session_lessons_nothing_to_inject",
    source_finding_ids_are_lesson_ids: true,
};

const SELF_IDENTITY_CODES: StageManifestCodes = StageManifestCodes {
    stage: "self_identity",
    policy_id: "self_identity",
    delivered_out_of_band: "self_identity_delivered_out_of_band",
    nothing_to_inject: "self_identity_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

const AGENT_LESSONS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "agent_lessons",
    policy_id: "agent_lessons",
    delivered_out_of_band: "agent_lessons_delivered_out_of_band",
    nothing_to_inject: "agent_lessons_nothing_to_inject",
    source_finding_ids_are_lesson_ids: true,
};

const DIAGNOSTIC_REMINDERS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "reminders",
    policy_id: "reminders",
    delivered_out_of_band: "diagnostic_reminder_delivered_out_of_band",
    nothing_to_inject: "diagnostic_reminder_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

pub(super) fn record_outcome_without_body_change(
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

pub(crate) struct SessionLessonsStage;

#[async_trait::async_trait]
impl RequestStage for SessionLessonsStage {
    fn name(&self) -> &'static str {
        "lessons"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.reminders_inject_lessons && is_session_own_lane(ctx)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if crate::lesson_delivery::client_holds_lessons(
            ctx.state.feature_toggle_overlay.load().mod_lesson_delivery,
            ctx.lesson_snapshot,
            ctx.session_id,
            body.body(),
        ) {
            body.record_unchanged(
                "session_lessons",
                "session_lessons",
                "session_lessons_held_by_client",
            );
            return;
        }
        let cfg = RemindersConfig {
            enabled: ctx.session_toggles.reminders,
            inject_lessons: ctx.session_toggles.reminders_inject_lessons,
            max_lessons_to_inject: ctx.session_toggles.reminders_max_lessons_to_inject,
        };
        let push_deps = ctx.state.push_deps();
        let outcome = reminders::apply_session_lessons(SessionLessonsRequest {
            body_bytes: body.body(),
            session_id: ctx.session_id,
            project_path: ctx.project_path,
            is_tool_loop: ctx.is_tool_loop,
            turn_origin: ctx.turn_origin,
            snapshot: ctx.lesson_snapshot,
            cfg: &cfg,
            recall_injector_enabled: ctx.session_toggles.recall_injector,
            request_id: ctx.request_id,
            push_deps: &push_deps,
        })
        .await;
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            source_finding_ids: injected_lesson_ids,
            ..
        } = outcome
        else {
            record_outcome_without_body_change(body, &SESSION_LESSONS_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            lesson_reminder_len = reminder_text.len(),
            lesson_ids = ?injected_lesson_ids,
            "lessons injected <system-reminder> block at session start"
        );
        body.merge_lesson_ids(injected_lesson_ids);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "session_lessons",
                policy_id: "session_lessons",
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "reminder_text_len": reminder_text.len() }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "session_lessons_injected",
                detail: Some(format!(
                    "injected {} bytes of lesson text",
                    reminder_text.len()
                )),
            },
        );
    }
}

pub(crate) struct SelfIdentityStage;

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

pub(crate) struct AgentLessonsStage;

#[async_trait::async_trait]
impl RequestStage for AgentLessonsStage {
    fn name(&self) -> &'static str {
        "agent_lessons"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.agent_lessons && matches!(ctx.turn_origin, TurnOrigin::Subagent)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let push_deps = ctx.state.push_deps();
        let outcome = reminders::apply_agent_lessons(reminders::AgentLessonsRequest {
            body_bytes: body.body(),
            session_id: ctx.session_id,
            subagent_persona: ctx.subagent_persona,
            turn_origin: ctx.turn_origin,
            snapshot: ctx.lesson_snapshot,
            agent_lessons_enabled: ctx.session_toggles.agent_lessons,
            request_id: ctx.request_id,
            push_deps: &push_deps,
        })
        .await;
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            source_finding_ids: injected_lesson_ids,
            ..
        } = outcome
        else {
            record_outcome_without_body_change(body, &AGENT_LESSONS_CODES, &outcome);
            return;
        };
        let new_bytes = Bytes::from(new_body);
        tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            persona = ?ctx.subagent_persona,
            agent_lesson_reminder_len = reminder_text.len(),
            lesson_ids = ?injected_lesson_ids,
            "agent-scoped lessons injected <system-reminder> block"
        );
        body.merge_lesson_ids(injected_lesson_ids);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "agent_lessons",
                policy_id: "agent_lessons",
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "persona": ctx.subagent_persona }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "agent_lessons_injected",
                detail: Some(format!(
                    "injected {} bytes of lesson text",
                    reminder_text.len()
                )),
            },
        );
    }
}

pub(crate) struct RemindersStage;

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
