use bytes::Bytes;

use crate::config::AppState;
use crate::extract::signals::RequestSignals;
use crate::feature_toggles::ResolvedToggles;
use crate::pipeline;
use crate::steerer;
use crate::store::models::{RequestId, SystemReminder};
use crate::wire::manifest::{ManifestBuilder, sha256_hex};

pub(super) struct MutationStageParams<'a> {
    pub(super) state: &'a AppState,
    pub(super) body_bytes: &'a Bytes,
    pub(super) session_toggles: &'a ResolvedToggles,
    pub(super) signals: &'a RequestSignals,
    pub(super) request_id: RequestId,
}

pub(super) struct MutationOutcome {
    pub(super) post_filter_body: Bytes,
    pub(super) filter_applied: bool,
    pub(super) firing_rule_ids: Vec<i64>,
    pub(super) injected_lesson_ids: Vec<i64>,
    pub(super) manifest: ManifestBuilder,
    pub(super) injected_reminders: Vec<SystemReminder>,
    pub(super) proxy_tools_declared: bool,
    pub(super) context_focus_asked: bool,
}

fn exempt_outcome(body_bytes: &Bytes) -> MutationOutcome {
    MutationOutcome {
        post_filter_body: body_bytes.clone(),
        filter_applied: false,
        firing_rule_ids: Vec::new(),
        injected_lesson_ids: Vec::new(),
        manifest: ManifestBuilder::new(sha256_hex(body_bytes.as_ref())),
        injected_reminders: Vec::new(),
        proxy_tools_declared: false,
        context_focus_asked: false,
    }
}

pub(super) async fn run_mutation_stages(params: MutationStageParams<'_>) -> MutationOutcome {
    let MutationStageParams {
        state,
        body_bytes,
        session_toggles,
        signals,
        request_id,
    } = params;

    if signals.mutation_exempt {
        return exempt_outcome(body_bytes);
    }

    let session_id = signals.session_id.as_str();
    let turn_origin = &signals.turn_origin;

    let filter_cfg = state.filter_config.load_full();
    let lesson_snapshot = state.lesson_cache.load();
    let lane = crate::injected_replay::lane_key(signals);
    let mut body = pipeline::StageBody::new(body_bytes.clone(), sha256_hex(body_bytes.as_ref()));
    crate::injected_replay::replay_into(&state.injected_replay, &lane, &mut body);
    let after_replay = body.body().clone();
    let stage_ctx = pipeline::StageContext {
        state,
        session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id,
        project_path: &signals.project_path,
        turn_origin,
        request_id,
        is_tool_loop: signals.is_tool_loop,
        subagent_persona: signals.subagent_persona.as_deref(),
        has_agent_id: signals.claude_code_agent_id.is_some(),
    };
    pipeline::run(&pipeline::stages_before_steerer(), &stage_ctx, &mut body).await;

    let pipeline::StageBodyParts {
        original,
        body: mut post_filter_body,
        mut filtered_stored,
        mut filter_applied,
        firing_rule_ids,
        injected_lesson_ids,
        mut manifest,
    } = body.into_parts();

    let injected_reminders = {
        let mut bk = crate::reminders::inject::InjectBookkeeping {
            post_filter_body: &mut post_filter_body,
            filtered_stored: &mut filtered_stored,
            filter_applied: &mut filter_applied,
            request_id,
        };
        steerer::stage::drain(
            state,
            session_id,
            turn_origin,
            &signals.human_prompt,
            &mut bk,
            &mut manifest,
        )
        .await
    };
    if post_filter_body != after_replay {
        state
            .injected_replay
            .record(&lane, &original, &post_filter_body);
    }

    let mut body = pipeline::StageBody::from_parts(pipeline::StageBodyParts {
        original,
        body: post_filter_body,
        filtered_stored,
        filter_applied,
        firing_rule_ids,
        injected_lesson_ids,
        manifest,
    });
    pipeline::run(&pipeline::stages_after_steerer(), &stage_ctx, &mut body).await;
    let pipeline::StageBodyParts {
        original: _,
        body: post_filter_body,
        filtered_stored: _filtered_stored,
        filter_applied,
        firing_rule_ids,
        injected_lesson_ids,
        manifest,
    } = body.into_parts();

    MutationOutcome {
        post_filter_body,
        filter_applied,
        firing_rule_ids,
        injected_lesson_ids,
        proxy_tools_declared: manifest.applied("proxy_tools"),
        context_focus_asked: manifest.applied(crate::context_focus::STAGE),
        manifest,
        injected_reminders,
    }
}
