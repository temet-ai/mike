use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use crate::compose_prompt::ComposerService;
use crate::feature_toggles::SessionToggleCache;
use crate::feature_toggles::service::FeatureToggleService;
use crate::filter::service::FilterService;
use crate::settings::service::SettingsService;
use crate::steerer::registry::SteererRegistry;
use crate::steerer::service::SteeringService;
use crate::store::persistence::workspace::RegisteredWorkspaceSession;

pub use crate::base_state::{BaseState, MailPush, NoMailPush, PushFuture};

pub mod accessors;
pub mod construction;
pub mod construction_llm;
pub mod construction_pipeline;
pub mod construction_services;
pub mod knowledge_state;
pub mod launch_state;
pub mod mail_push;
#[cfg(test)]
mod tests;

pub use construction_services::fold_db_settings;

#[cfg(any(test, feature = "test-seams"))]
pub mod test_seams;
#[cfg(any(test, feature = "test-seams"))]
pub use test_seams::register_in_flight_for_tests;

pub mod client_serves;
pub use client_serves::client_serves;
#[cfg(any(test, feature = "test-seams"))]
pub use client_serves::serve_every_stage_for_tests;

#[derive(Clone)]
pub struct AppState {
    pub base: BaseState,
    pub remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub remote_control_writers: crate::proxy::remote_control::capture::CallWriters,
    pub analyzer_registry: Arc<ArcSwap<AnalyzerRegistry>>,
    pub filter_config: Arc<ArcSwap<crate::filter::FilterConfig>>,
    pub filter_service: Option<Arc<FilterService>>,
    pub system_replay: Arc<crate::filter::replay::ReplayRegistry>,
    pub injected_replay: Arc<crate::injected_replay::InjectedBlockRegistry>,
    pub composer: Arc<ComposerService>,
    pub embedding_service: EmbeddingService,
    pub rerank_service: crate::analysis::RerankService,
    pub semantic: crate::semantic::SemanticHandle,
    pub shed_registry: Arc<crate::memory_cap::ShedRegistry>,
    pub capture_admission_throttled: Arc<std::sync::atomic::AtomicBool>,
    pub lesson_cache: crate::lesson_worker::LessonCache,
    pub keeper_snapshot: crate::memory_keeper::snapshot::KeeperCache,
    pub poll_answers: crate::api::poll_answers::PollAnswers,

    pub steerer_registry: Arc<ArcSwap<SteererRegistry>>,

    pub steerer_service: Option<Arc<SteeringService>>,

    pub feature_toggle_service: Option<Arc<FeatureToggleService>>,

    pub settings_service: Option<Arc<SettingsService>>,

    pub session_toggle_cache: SessionToggleCache,

    pub channel_link_attempted: Arc<crate::channel_link_sweep::link::ChannelLinkProgress>,
    pub sessions_with_mike_tools: Arc<crate::proxy_tools::session::SessionsWithMikeTools>,

    pub injection_gate: Arc<crate::injection::InjectionGate>,

    pub workspace_registration_cells:
        Arc<dashmap::DashMap<String, Arc<tokio::sync::OnceCell<RegisteredWorkspaceSession>>>>,

    pub workspace_briefing_delivered: Arc<dashmap::DashSet<String>>,

    pub workspace_briefing_served: Arc<dashmap::DashSet<String>>,

    pub self_identity_served: Arc<dashmap::DashSet<String>>,

    pub change_record_tail_served: Arc<dashmap::DashSet<String>>,

    pub model_catalogue: Arc<crate::steerer::model_catalogue::ModelCatalogue>,

    pub openrouter_usage_cache: Arc<crate::api::usage::OpenRouterUsageCache>,

    pub resync_status: crate::resync::ResyncStatusHandle,

    pub judge_llm: Option<crate::llm::OpenRouterClient>,

    pub anthropic_api_key: crate::llm::key::AnthropicKeySlot,

    pub service_token: crate::service_token::ServiceToken,

    pub anthropic_cli: crate::anthropic_cli::AnthropicCliState,
}

impl std::ops::Deref for AppState {
    type Target = BaseState;

    fn deref(&self) -> &BaseState {
        &self.base
    }
}

impl std::ops::DerefMut for AppState {
    fn deref_mut(&mut self) -> &mut BaseState {
        &mut self.base
    }
}
