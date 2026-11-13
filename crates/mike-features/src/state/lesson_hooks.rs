use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use bytes::Bytes;

use crate::config::memory_harvester::HarvesterConfig;
use crate::pipeline::RequestStage;
use crate::quiescence_worker::sweep::SweepContext;
use crate::rerank_nightly::PairsPhase;
use crate::store::models::RequestId;
use crate::store::updater::ResponseHook;
use crate::wire::manifest::ManifestBuilder;

use super::AppState;

pub type HookFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonStage {
    RecallInjector,
    SessionLessons,
    MemoryKeeper,
    AgentLessons,
}

pub struct SupplyCapture<'a> {
    pub request_id: RequestId,
    pub session_id: &'a str,
    pub original: &'a Bytes,
    pub sent: &'a Bytes,
    pub proxy_lessons: &'a [i64],
}

pub struct LessonAttribution<'a> {
    pub session_id: &'a str,
    pub lane: &'static str,
    pub persona: Option<String>,
    pub injected: Vec<i64>,
    pub mod_lessons: Vec<i64>,
    pub request_id: RequestId,
}

pub trait LessonHooks: Send + Sync {
    fn stage(&self, slot: LessonStage) -> Option<Box<dyn RequestStage>>;

    fn pairs_phase(&self) -> Option<Arc<dyn PairsPhase>>;

    fn response_hook(&self, config: HarvesterConfig) -> Option<Arc<dyn ResponseHook>>;

    fn label_mod_lessons(&self, body: &[u8], manifest: &mut ManifestBuilder) -> Vec<i64>;

    fn spawn_supply_records(&self, state: &AppState, capture: SupplyCapture<'_>);

    fn attribute(&self, state: &AppState, attribution: LessonAttribution<'_>);

    fn proposer_tick<'a>(
        &'a self,
        ctx: &'a SweepContext<'a>,
        tick: u64,
        toggle_on: bool,
    ) -> HookFuture<'a>;
}

pub struct NoLessonHooks;

impl LessonHooks for NoLessonHooks {
    fn stage(&self, _slot: LessonStage) -> Option<Box<dyn RequestStage>> {
        None
    }

    fn pairs_phase(&self) -> Option<Arc<dyn PairsPhase>> {
        None
    }

    fn response_hook(&self, _config: HarvesterConfig) -> Option<Arc<dyn ResponseHook>> {
        None
    }

    fn label_mod_lessons(&self, _body: &[u8], _manifest: &mut ManifestBuilder) -> Vec<i64> {
        Vec::new()
    }

    fn spawn_supply_records(&self, _state: &AppState, _capture: SupplyCapture<'_>) {}

    fn attribute(&self, _state: &AppState, _attribution: LessonAttribution<'_>) {}

    fn proposer_tick<'a>(
        &'a self,
        _ctx: &'a SweepContext<'a>,
        _tick: u64,
        _toggle_on: bool,
    ) -> HookFuture<'a> {
        Box::pin(async {})
    }
}

static NO_LESSON_HOOKS: NoLessonHooks = NoLessonHooks;

pub type LessonHookCell = Arc<OnceLock<Arc<dyn LessonHooks>>>;

pub fn hooks_in(cell: &LessonHookCell) -> &dyn LessonHooks {
    cell.get()
        .map_or(&NO_LESSON_HOOKS as &dyn LessonHooks, |hooks| hooks.as_ref())
}

impl AppState {
    pub fn lesson_hooks(&self) -> &dyn LessonHooks {
        hooks_in(&self.lesson_hooks)
    }

    pub fn shared_lesson_hooks(&self) -> Option<Arc<dyn LessonHooks>> {
        self.lesson_hooks.get().cloned()
    }

    pub fn install_lesson_hooks(&self, hooks: Arc<dyn LessonHooks>) {
        let _ = self.lesson_hooks.set(hooks);
    }
}
