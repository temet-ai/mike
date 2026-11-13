use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64};

use arc_swap::ArcSwap;
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use crate::compose_prompt::ComposerService;
use crate::feature_toggles::service::FeatureToggleService;
use crate::feature_toggles::{FeatureToggles, SessionToggleCache};
use crate::filter::service::FilterService;
use crate::llm::LlmUsageSink;
use crate::settings::SettingsOverlay;
use crate::settings::service::SettingsService;
use crate::steerer::registry::SteererRegistry;
use crate::steerer::service::SteeringService;
use crate::store::RequestStore;
use crate::store::models::ProxyEvent;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::RegisteredWorkspaceSession;
use crate::store::retention::RetentionPolicy;

use crate::config::ProxyConfig;

pub mod accessors;
pub mod construction;
pub mod construction_llm;
pub mod construction_pipeline;
pub mod construction_services;
#[cfg(test)]
pub mod tests;

pub use construction_services::fold_db_settings;

#[cfg(any(test, feature = "test-seams"))]
pub mod test_seams;
#[cfg(any(test, feature = "test-seams"))]
pub use test_seams::register_in_flight_for_tests;

pub mod client_serves;
pub use client_serves::client_serves;
#[cfg(test)]
pub use client_serves::serve_every_stage_for_tests;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<RwLock<RequestStore>>,
    pub client: reqwest::Client,
    pub event_tx: broadcast::Sender<ProxyEvent>,
    pub config: Arc<ProxyConfig>,
    pub shutdown: CancellationToken,
    pub tracker: TaskTracker,
    pub remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub remote_control_writers: crate::proxy::remote_control::capture::CallWriters,
    pub retention: RetentionPolicy,
    pub request_count: Arc<AtomicU64>,
    pub last_request_at_unix: Arc<AtomicI64>,
    pub last_capturable_request_at_unix: Arc<AtomicI64>,
    pub started_at: std::time::Instant,
    pub metrics_handle: PrometheusHandle,
    pub analyzer_registry: Arc<ArcSwap<AnalyzerRegistry>>,
    pub persist: Option<PersistHandle>,
    pub filter_config: Arc<ArcSwap<crate::filter::FilterConfig>>,
    pub filter_service: Option<Arc<FilterService>>,
    pub system_replay: Arc<crate::filter::replay::ReplayRegistry>,
    pub injected_replay: Arc<crate::injected_replay::InjectedBlockRegistry>,
    pub composer: Arc<ComposerService>,
    pub embedding_service: EmbeddingService,
    pub rerank_service: crate::analysis::RerankService,
    pub semantic: crate::semantic::SemanticHandle,
    pub in_flight: Arc<crate::in_flight::InFlightRegistry>,
    pub shed_registry: Arc<crate::memory_cap::ShedRegistry>,
    pub capture_admission_throttled: Arc<std::sync::atomic::AtomicBool>,
    pub lesson_cache: crate::lesson_worker::LessonCache,
    pub keeper_snapshot: crate::memory_keeper::snapshot::KeeperCache,
    pub change_feed: crate::change_feed::ChangeFeed,
    pub poll_answers: crate::api::poll_answers::PollAnswers,

    pub steerer_registry: Arc<ArcSwap<SteererRegistry>>,

    pub steerer_service: Option<Arc<SteeringService>>,

    pub feature_toggle_overlay: Arc<ArcSwap<FeatureToggles>>,

    pub feature_toggle_service: Option<Arc<FeatureToggleService>>,

    pub settings_overlay: Arc<ArcSwap<SettingsOverlay>>,

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

    pub host_machine_uuid: Arc<tokio::sync::OnceCell<uuid::Uuid>>,

    pub model_catalogue: Arc<crate::steerer::model_catalogue::ModelCatalogue>,

    pub openrouter_usage_cache: Arc<crate::api::usage::OpenRouterUsageCache>,

    pub resync_status: crate::resync::ResyncStatusHandle,

    pub judge_llm: Option<crate::llm::OpenRouterClient>,

    pub llm_usage_sink: Option<Arc<dyn LlmUsageSink>>,

    pub steerer_openrouter_key: crate::llm::key::OpenRouterKeySlot,

    pub anthropic_api_key: crate::llm::key::AnthropicKeySlot,

    pub service_token: crate::service_token::ServiceToken,

    pub anthropic_cli: crate::anthropic_cli::AnthropicCliState,
}
