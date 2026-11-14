use std::sync::Arc;

use crate::config::ProxyConfig;
use crate::lessons_settings::HarvesterConfig;
use crate::pipeline::RequestStage;
use crate::quiescence_worker::sweep::SweepContext;
use crate::rerank_nightly::PairsPhase;
use crate::state::AppState;
use crate::state::lesson_hooks::{
    HookFuture, LessonAttribution, LessonHooks, LessonStage, SupplyCapture,
};
use crate::store::updater::ResponseHook;
use crate::wire::manifest::ManifestBuilder;

pub struct Lessons;

impl LessonHooks for Lessons {
    fn stage(&self, slot: LessonStage) -> Option<Box<dyn RequestStage>> {
        Some(match slot {
            LessonStage::RecallInjector => {
                Box::new(crate::recall_injector::stage::RecallInjectorStage)
            }
            LessonStage::SessionLessons => {
                Box::new(crate::reminders::session_lessons_stage::SessionLessonsStage)
            }
            LessonStage::MemoryKeeper => Box::new(crate::memory_keeper::stage::MemoryKeeperStage),
            LessonStage::AgentLessons => {
                Box::new(crate::reminders::agent_lessons_stage::AgentLessonsStage)
            }
        })
    }

    fn pairs_phase(&self) -> Option<Arc<dyn PairsPhase>> {
        Some(Arc::new(crate::rerank_nightly::lesson_pairs::LessonPairs))
    }

    fn response_hook(
        &self,
        config: &ProxyConfig,
        enabled: Option<bool>,
    ) -> Option<Arc<dyn ResponseHook>> {
        let mut harvester = config.sections.get::<HarvesterConfig>();
        if let Some(enabled) = enabled {
            harvester.enabled = enabled;
        }
        Some(Arc::new(crate::memory_harvester::HarvesterHook::new(
            harvester,
        )))
    }

    fn label_mod_lessons(&self, body: &[u8], manifest: &mut ManifestBuilder) -> Vec<i64> {
        super::label_mod_lessons(body, manifest)
    }

    fn spawn_supply_records(&self, state: &AppState, capture: SupplyCapture<'_>) {
        super::supplied::spawn_supply_records(state, capture);
    }

    fn attribute(&self, state: &AppState, attribution: LessonAttribution<'_>) {
        super::attribution::spawn_attribution(state, attribution);
    }

    fn proposer_tick<'a>(
        &'a self,
        ctx: &'a SweepContext<'a>,
        tick: u64,
        toggle_on: bool,
    ) -> HookFuture<'a> {
        Box::pin(async move {
            if crate::quiescence_worker::proposer_tick::is_proposer_tick(tick) {
                crate::quiescence_worker::proposer_tick::run_proposer_tick(ctx, toggle_on).await;
            }
        })
    }
}

pub fn install(state: &AppState) {
    state.install_lesson_hooks(Arc::new(Lessons));
}
