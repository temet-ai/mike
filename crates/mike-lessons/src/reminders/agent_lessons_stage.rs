use bytes::Bytes;

use crate::extract::request::TurnOrigin;
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::reminders::RemindersOutcome;
use crate::reminders::agent_lessons::{AgentLessonsRequest, apply_agent_lessons_with_floor};
use crate::reminders::stage::{StageManifestCodes, record_outcome_without_body_change};
use crate::wire::manifest::AppliedStage;

const AGENT_LESSONS_CODES: StageManifestCodes = StageManifestCodes {
    stage: "agent_lessons",
    policy_id: "agent_lessons",
    delivered_out_of_band: "agent_lessons_delivered_out_of_band",
    nothing_to_inject: "agent_lessons_nothing_to_inject",
    source_finding_ids_are_lesson_ids: true,
};

pub struct AgentLessonsStage;

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
        let floor = ctx
            .state
            .decider(crate::reminders::supply_floor::POINT)
            .map(crate::llm::decisions::point::Decider::inline);
        let outcome = apply_agent_lessons_with_floor(
            AgentLessonsRequest {
                body_bytes: body.body(),
                session_id: ctx.session_id,
                subagent_persona: ctx.subagent_persona,
                turn_origin: ctx.turn_origin,
                snapshot: ctx.lesson_snapshot,
                agent_lessons_enabled: ctx.session_toggles.agent_lessons,
                request_id: ctx.request_id,
                push_deps: &push_deps,
            },
            floor.as_ref(),
        )
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
