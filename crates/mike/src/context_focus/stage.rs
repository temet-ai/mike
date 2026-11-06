use crate::compress::manifest::AppliedStage;
use crate::context_focus::inject::{self, InjectOutcome};
use crate::context_focus::{FIELD, STAGE};
use crate::pipeline::{RequestStage, StageBody, StageContext};

pub(crate) struct ContextFocusStage;

#[async_trait::async_trait]
impl RequestStage for ContextFocusStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.context_focus_question
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        if !ctx.turn_origin.resends_the_session_tool_list() {
            body.decline(
                STAGE,
                "context_focus_sidecar_call",
                serde_json::json!({ "turn_origin": ctx.turn_origin.to_string() }),
            );
            return;
        }
        let mode = inject::Mode::parse(&ctx.session_toggles.context_focus_question_mode);
        let InjectOutcome::Injected {
            new_body,
            tools,
            strict_tools,
        } = inject::inject(body.body(), mode)
        else {
            body.decline(
                STAGE,
                "context_focus_no_tool_to_extend",
                serde_json::json!({}),
            );
            return;
        };
        let detail = format!(
            "asked for {FIELD} on {} tools ({} mode, {} strict)",
            tools.len(),
            mode.as_str(),
            strict_tools.len()
        );
        body.rewrite(
            new_body.clone(),
            AppliedStage {
                stage: STAGE,
                policy_id: STAGE,
                policy_version: 2,
                resolved_parameters: serde_json::json!({
                    "field": FIELD,
                    "max_chars": inject::MAX_CHARS,
                    "mode": mode.as_str(),
                    "tools": tools,
                    "strict_tools": strict_tools,
                }),
                new_bytes: new_body.as_ref(),
                targets: Vec::new(),
                reason_code: "context_focus_field_added",
                detail: Some(detail),
            },
        );
    }
}
