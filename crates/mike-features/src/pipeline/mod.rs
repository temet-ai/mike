use bytes::Bytes;

use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_cache::LessonSnapshot;
use crate::state::AppState;
use crate::state::lesson_hooks::{LessonHooks, LessonStage};
use crate::store::models::RequestId;
use crate::wire::manifest::{AppliedStage, ManifestBuilder, UnchangedStage};

pub fn absorb_body_rewrite(
    post_filter_body: &mut Bytes,
    filtered_stored: &mut Option<Bytes>,
    filter_applied: &mut bool,
    new_bytes: Bytes,
) {
    *post_filter_body = new_bytes.clone();
    *filtered_stored = Some(new_bytes);
    *filter_applied = true;
}

pub struct StageBody {
    original: Bytes,
    body: Bytes,
    filtered_stored: Option<Bytes>,
    filter_applied: bool,
    firing_rule_ids: Vec<i64>,
    injected_lesson_ids: Vec<i64>,
    manifest: ManifestBuilder,
}

impl StageBody {
    pub fn new(body: Bytes, original_sha256: String) -> Self {
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

    pub fn body(&self) -> &Bytes {
        &self.body
    }

    pub fn original(&self) -> &Bytes {
        &self.original
    }

    pub fn filtered_stored(&self) -> Option<&Bytes> {
        self.filtered_stored.as_ref()
    }

    pub fn filter_applied(&self) -> bool {
        self.filter_applied
    }

    pub fn firing_rule_ids(&self) -> &[i64] {
        &self.firing_rule_ids
    }

    pub fn injected_lesson_ids(&self) -> &[i64] {
        &self.injected_lesson_ids
    }

    pub fn rewrite(&mut self, new_bytes: Bytes, stage: AppliedStage<'_>) {
        self.manifest.record_applied(stage);
        absorb_body_rewrite(
            &mut self.body,
            &mut self.filtered_stored,
            &mut self.filter_applied,
            new_bytes,
        );
    }

    pub fn record_unchanged(&mut self, stage: &str, policy_id: &str, reason_code: &str) {
        self.manifest
            .record_unchanged(stage, policy_id, reason_code);
    }

    pub fn decline(
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

    pub fn decline_with(&mut self, unchanged: UnchangedStage<'_>) {
        self.manifest.record_unchanged_with(unchanged);
    }

    pub fn rewrite_forwarded_only(&mut self, new_bytes: Bytes, stage: AppliedStage<'_>) {
        self.manifest.record_applied(stage);
        self.body = new_bytes;
        self.filter_applied = true;
    }

    pub fn absorb(&mut self, new_bytes: Bytes) {
        absorb_body_rewrite(
            &mut self.body,
            &mut self.filtered_stored,
            &mut self.filter_applied,
            new_bytes,
        );
    }

    pub fn merge_rule_ids(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            if !self.firing_rule_ids.contains(&id) {
                self.firing_rule_ids.push(id);
            }
        }
    }

    pub fn merge_lesson_ids(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            if !self.injected_lesson_ids.contains(&id) {
                self.injected_lesson_ids.push(id);
            }
        }
    }

    pub fn into_parts(self) -> StageBodyParts {
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

    pub fn from_parts(parts: StageBodyParts) -> Self {
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

pub struct StageBodyParts {
    pub original: Bytes,
    pub body: Bytes,
    pub filtered_stored: Option<Bytes>,
    pub filter_applied: bool,
    pub firing_rule_ids: Vec<i64>,
    pub injected_lesson_ids: Vec<i64>,
    pub manifest: ManifestBuilder,
}

pub struct StageContext<'a> {
    pub state: &'a AppState,
    pub session_toggles: &'a ResolvedToggles,
    pub filter_cfg: &'a FilterConfig,
    pub lesson_snapshot: &'a LessonSnapshot,
    pub session_id: &'a str,
    pub project_path: &'a str,
    pub turn_origin: &'a TurnOrigin,
    pub request_id: RequestId,
    pub is_tool_loop: bool,
    pub subagent_persona: Option<&'a str>,
    pub has_agent_id: bool,
}

#[async_trait::async_trait]
pub trait RequestStage: Send + Sync {
    fn name(&self) -> &'static str;

    fn enabled(&self, ctx: &StageContext<'_>) -> bool;

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody);
}

pub fn is_session_own_lane(ctx: &StageContext<'_>) -> bool {
    matches!(
        ctx.turn_origin,
        TurnOrigin::Primary | TurnOrigin::PrimaryFirst
    ) && !ctx.has_agent_id
}

pub const STAGE_ORDER: [&str; 20] = [
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
    "code_findings",
    "git_read_hint",
    "steerer",
    "repeated_notices",
    "read_dedup",
    "file_read_compression",
    "shell_output_compression",
    "proxy_tools",
    "context_focus_question",
];

pub fn registry(lessons: &dyn LessonHooks) -> Vec<Box<dyn RequestStage>> {
    let mut all = stages_before_steerer(lessons);
    all.extend(stages_after_steerer());
    all
}

pub fn stages_before_steerer(lessons: &dyn LessonHooks) -> Vec<Box<dyn RequestStage>> {
    let stages: Vec<Option<Box<dyn RequestStage>>> = vec![
        Some(Box::new(crate::filter::stage::FilterStage)),
        Some(Box::new(
            crate::reminder_on_token::stage::ReminderOnTokenStage,
        )),
        Some(Box::new(crate::lazy_sections::stage::LazySectionsStage)),
        lessons.stage(LessonStage::RecallInjector),
        lessons.stage(LessonStage::SessionLessons),
        lessons.stage(LessonStage::MemoryKeeper),
        Some(Box::new(crate::reminders::stage::SelfIdentityStage)),
        Some(Box::new(
            crate::reminders::workspace_briefing::WorkspaceBriefingStage,
        )),
        lessons.stage(LessonStage::AgentLessons),
        Some(Box::new(crate::reminders::stage::RemindersStage)),
        Some(Box::new(
            crate::reminders::change_record_tail::ChangeRecordTailStage,
        )),
        Some(Box::new(crate::reminders::code_findings::CodeFindingsStage)),
        Some(Box::new(crate::reminders::git_read_hint::GitReadHintStage)),
    ];
    stages.into_iter().flatten().collect()
}

pub fn stages_after_steerer() -> Vec<Box<dyn RequestStage>> {
    vec![
        Box::new(crate::repeated_notices::stage::RepeatedNoticesStage),
        Box::new(crate::read_dedup::stage::ReadDedupStage),
        Box::new(crate::compress::stage::FileReadCompressionStage),
        Box::new(crate::compress::shell_stage::ShellOutputCompressionStage),
        Box::new(crate::proxy_tools::stage::ProxyToolsStage),
        Box::new(crate::context_focus::stage::ContextFocusStage),
    ]
}

pub async fn run(stages: &[Box<dyn RequestStage>], ctx: &StageContext<'_>, body: &mut StageBody) {
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
pub mod stage_body_tests;

#[cfg(test)]
pub mod registry_tests;
