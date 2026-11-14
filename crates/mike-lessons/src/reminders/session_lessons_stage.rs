use bytes::Bytes;

use crate::features_settings::RemindersOverlay;
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::reminders::session_lessons::{SessionLessonsRequest, apply_session_lessons};
use crate::reminders::stage::{StageManifestCodes, record_outcome_without_body_change};
use crate::reminders::{RemindersConfig, RemindersOutcome};
use crate::wire::manifest::AppliedStage;

const SESSION_LESSONS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "session_lessons",
    policy_id: "session_lessons",
    delivered_out_of_band: "session_lessons_delivered_out_of_band",
    nothing_to_inject: "session_lessons_nothing_to_inject",
    source_finding_ids_are_lesson_ids: true,
};

pub struct SessionLessonsStage;

#[async_trait::async_trait]
impl RequestStage for SessionLessonsStage {
    fn name(&self) -> &'static str {
        "lessons"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles
            .features
            .get::<RemindersOverlay>()
            .inject_lessons
            && is_session_own_lane(ctx)
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
        let reminders = ctx.session_toggles.features.get::<RemindersOverlay>();
        let cfg = RemindersConfig {
            enabled: ctx.session_toggles.reminders,
            inject_lessons: reminders.inject_lessons,
            max_lessons_to_inject: reminders.max_lessons_to_inject,
        };
        let push_deps = ctx.state.push_deps();
        let outcome = apply_session_lessons(SessionLessonsRequest {
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
