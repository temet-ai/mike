use super::*;
use crate::config::memory_harvester::HarvesterConfig;
use crate::quiescence_worker::sweep::SweepContext;
use crate::rerank_nightly::PairsPhase;
use crate::state::lesson_hooks::{HookFuture, LessonAttribution, SupplyCapture};
use crate::store::updater::ResponseHook;
use crate::wire::manifest::ManifestBuilder;
use std::sync::Arc;

struct Named(&'static str);

#[async_trait::async_trait]
impl RequestStage for Named {
    fn name(&self) -> &'static str {
        self.0
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        false
    }

    async fn apply(&self, _ctx: &StageContext<'_>, _body: &mut StageBody) {}
}

struct NamedLessons;

impl LessonHooks for NamedLessons {
    fn stage(&self, slot: LessonStage) -> Option<Box<dyn RequestStage>> {
        Some(Box::new(Named(match slot {
            LessonStage::RecallInjector => "recall_injector",
            LessonStage::SessionLessons => "lessons",
            LessonStage::MemoryKeeper => "memory_keeper",
            LessonStage::AgentLessons => "agent_lessons",
        })))
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

#[test]
fn registry_follows_the_documented_stage_order() {
    let names: Vec<&str> = registry(&NamedLessons).iter().map(|s| s.name()).collect();
    let expected: Vec<&str> = STAGE_ORDER
        .iter()
        .copied()
        .filter(|n| *n != "steerer")
        .collect();
    assert_eq!(names, expected, "registry order drifted from STAGE_ORDER");
}

#[test]
fn registry_excludes_the_explicitly_invoked_stages() {
    let names: Vec<&str> = registry(&NamedLessons).iter().map(|s| s.name()).collect();
    assert!(!names.contains(&"steerer"), "steerer is invoked by name");
    assert!(!names.contains(&"composer"), "composer is invoked by name");
}
