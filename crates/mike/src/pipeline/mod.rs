#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "StageBody's read accessors, registry(), and RequestStage::name are exercised only by this crate's own tests (this module's + every stage.rs's), not by proxy_handler's production call sites"
    )
)]

use bytes::Bytes;

use crate::compress::manifest::{AppliedStage, ManifestBuilder, UnchangedStage};
use crate::config::AppState;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::store::models::RequestId;

pub(crate) fn absorb_body_rewrite(
    post_filter_body: &mut Bytes,
    filtered_stored: &mut Option<Bytes>,
    filter_applied: &mut bool,
    new_bytes: Bytes,
) {
    *post_filter_body = new_bytes.clone();
    *filtered_stored = Some(new_bytes);
    *filter_applied = true;
}

pub(crate) struct StageBody {
    original: Bytes,
    body: Bytes,
    filtered_stored: Option<Bytes>,
    filter_applied: bool,
    firing_rule_ids: Vec<i64>,
    injected_lesson_ids: Vec<i64>,
    manifest: ManifestBuilder,
}

impl StageBody {
    pub(crate) fn new(body: Bytes, original_sha256: String) -> Self {
        Self {
            original: body.clone(),
            body,
            filtered_stored: None,
            filter_applied: false,
            firing_rule_ids: Vec::new(),
            injected_lesson_ids: Vec::new(),
            manifest: ManifestBuilder::new(original_sha256),
        }
    }

    pub(crate) fn body(&self) -> &Bytes {
        &self.body
    }

    pub(crate) fn original(&self) -> &Bytes {
        &self.original
    }

    pub(crate) fn filtered_stored(&self) -> Option<&Bytes> {
        self.filtered_stored.as_ref()
    }

    pub(crate) fn filter_applied(&self) -> bool {
        self.filter_applied
    }

    pub(crate) fn firing_rule_ids(&self) -> &[i64] {
        &self.firing_rule_ids
    }

    pub(crate) fn injected_lesson_ids(&self) -> &[i64] {
        &self.injected_lesson_ids
    }

    pub(crate) fn rewrite(&mut self, new_bytes: Bytes, stage: AppliedStage<'_>) {
        self.manifest.record_applied(stage);
        absorb_body_rewrite(
            &mut self.body,
            &mut self.filtered_stored,
            &mut self.filter_applied,
            new_bytes,
        );
    }

    pub(crate) fn record_unchanged(&mut self, stage: &str, policy_id: &str, reason_code: &str) {
        self.manifest
            .record_unchanged(stage, policy_id, reason_code);
    }

    pub(crate) fn decline(
        &mut self,
        stage: &str,
        reason_code: &str,
        resolved_parameters: serde_json::Value,
    ) {
        self.manifest.record_unchanged_with(UnchangedStage {
            stage,
            policy_id: stage,
            reason_code,
            resolved_parameters,
            targets: Vec::new(),
            detail: None,
        });
    }

    pub(crate) fn decline_with(&mut self, unchanged: UnchangedStage<'_>) {
        self.manifest.record_unchanged_with(unchanged);
    }

    pub(crate) fn rewrite_forwarded_only(&mut self, new_bytes: Bytes, stage: AppliedStage<'_>) {
        self.manifest.record_applied(stage);
        self.body = new_bytes;
        self.filter_applied = true;
    }

    pub(crate) fn absorb(&mut self, new_bytes: Bytes) {
        absorb_body_rewrite(
            &mut self.body,
            &mut self.filtered_stored,
            &mut self.filter_applied,
            new_bytes,
        );
    }

    pub(crate) fn merge_rule_ids(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            if !self.firing_rule_ids.contains(&id) {
                self.firing_rule_ids.push(id);
            }
        }
    }

    pub(crate) fn merge_lesson_ids(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            if !self.injected_lesson_ids.contains(&id) {
                self.injected_lesson_ids.push(id);
            }
        }
    }

    pub(crate) fn into_parts(self) -> StageBodyParts {
        StageBodyParts {
            original: self.original,
            body: self.body,
            filtered_stored: self.filtered_stored,
            filter_applied: self.filter_applied,
            firing_rule_ids: self.firing_rule_ids,
            injected_lesson_ids: self.injected_lesson_ids,
            manifest: self.manifest,
        }
    }

    pub(crate) fn from_parts(parts: StageBodyParts) -> Self {
        Self {
            original: parts.original,
            body: parts.body,
            filtered_stored: parts.filtered_stored,
            filter_applied: parts.filter_applied,
            firing_rule_ids: parts.firing_rule_ids,
            injected_lesson_ids: parts.injected_lesson_ids,
            manifest: parts.manifest,
        }
    }
}

pub(crate) struct StageBodyParts {
    pub(crate) original: Bytes,
    pub(crate) body: Bytes,
    pub(crate) filtered_stored: Option<Bytes>,
    pub(crate) filter_applied: bool,
    pub(crate) firing_rule_ids: Vec<i64>,
    pub(crate) injected_lesson_ids: Vec<i64>,
    pub(crate) manifest: ManifestBuilder,
}

pub(crate) struct StageContext<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) session_toggles: &'a ResolvedToggles,
    pub(crate) filter_cfg: &'a FilterConfig,
    pub(crate) lesson_snapshot: &'a LessonSnapshot,
    pub(crate) session_id: &'a str,
    pub(crate) project_path: &'a str,
    pub(crate) turn_origin: &'a TurnOrigin,
    pub(crate) request_id: RequestId,
    pub(crate) is_tool_loop: bool,
    pub(crate) subagent_persona: Option<&'a str>,
    pub(crate) has_agent_id: bool,
}

#[async_trait::async_trait]
pub(crate) trait RequestStage: Send + Sync {
    fn name(&self) -> &'static str;

    fn enabled(&self, ctx: &StageContext<'_>) -> bool;

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody);
}

pub(crate) fn is_session_own_lane(ctx: &StageContext<'_>) -> bool {
    matches!(
        ctx.turn_origin,
        TurnOrigin::Primary | TurnOrigin::PrimaryFirst
    ) && !ctx.has_agent_id
}

pub(crate) const STAGE_ORDER: [&str; 19] = [
    "filter",
    "reminder_on_token",
    "lazy_sections",
    "recall_injector",
    "lessons",
    "memory_keeper",
    "self_identity",
    "workspace_briefing",
    "agent_lessons",
    "reminders",
    "change_record_tail",
    "git_read_hint",
    "steerer",
    "repeated_notices",
    "read_dedup",
    "file_read_compression",
    "shell_output_compression",
    "proxy_tools",
    "context_focus_question",
];

pub(crate) fn registry() -> Vec<Box<dyn RequestStage>> {
    let mut all = stages_before_steerer();
    all.extend(stages_after_steerer());
    all
}

pub(crate) fn stages_before_steerer() -> Vec<Box<dyn RequestStage>> {
    vec![
        Box::new(crate::filter::stage::FilterStage),
        Box::new(crate::reminder_on_token::stage::ReminderOnTokenStage),
        Box::new(crate::lazy_sections::stage::LazySectionsStage),
        Box::new(crate::recall_injector::stage::RecallInjectorStage),
        Box::new(crate::reminders::stage::SessionLessonsStage),
        Box::new(crate::memory_keeper::stage::MemoryKeeperStage),
        Box::new(crate::reminders::stage::SelfIdentityStage),
        Box::new(crate::reminders::workspace_briefing::WorkspaceBriefingStage),
        Box::new(crate::reminders::stage::AgentLessonsStage),
        Box::new(crate::reminders::stage::RemindersStage),
        Box::new(crate::reminders::change_record_tail::ChangeRecordTailStage),
        Box::new(crate::reminders::git_read_hint::GitReadHintStage),
    ]
}

pub(crate) fn stages_after_steerer() -> Vec<Box<dyn RequestStage>> {
    vec![
        Box::new(crate::repeated_notices::stage::RepeatedNoticesStage),
        Box::new(crate::read_dedup::stage::ReadDedupStage),
        Box::new(crate::compress::stage::FileReadCompressionStage),
        Box::new(crate::compress::shell_stage::ShellOutputCompressionStage),
        Box::new(crate::proxy_tools::stage::ProxyToolsStage),
        Box::new(crate::context_focus::stage::ContextFocusStage),
    ]
}

pub(crate) async fn run(
    stages: &[Box<dyn RequestStage>],
    ctx: &StageContext<'_>,
    body: &mut StageBody,
) {
    if is_session_own_lane(ctx) && !ctx.is_tool_loop {
        crate::explained::observe_body(ctx.session_id, body.body());
    }
    for stage in stages {
        if stage.enabled(ctx) {
            stage.apply(ctx, body).await;
        }
    }
}

#[cfg(test)]
mod stage_body_tests;

#[cfg(test)]
mod registry_tests;
