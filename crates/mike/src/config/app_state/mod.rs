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

use super::schema::ProxyConfig;

mod accessors;
mod construction;
mod construction_llm;
mod construction_pipeline;
mod construction_services;
#[cfg(test)]
mod tests;

pub(crate) use construction_services::fold_db_settings;

#[cfg(any(test, feature = "test-seams"))]
mod test_seams;
#[cfg(any(test, feature = "test-seams"))]
pub use test_seams::register_in_flight_for_tests;

#[derive(Clone)]
pub struct AppState {
    pub(crate) store: Arc<RwLock<RequestStore>>,
    pub(crate) client: reqwest::Client,
    pub(crate) event_tx: broadcast::Sender<ProxyEvent>,
    pub(crate) config: Arc<ProxyConfig>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) tracker: TaskTracker,
    pub(crate) remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub(crate) remote_control_writers: crate::proxy::remote_control::capture::CallWriters,
    pub(crate) retention: RetentionPolicy,
    pub(crate) request_count: Arc<AtomicU64>,
    pub(crate) last_request_at_unix: Arc<AtomicI64>,
    pub(crate) last_capturable_request_at_unix: Arc<AtomicI64>,
    pub(crate) started_at: std::time::Instant,
    pub metrics_handle: PrometheusHandle,
    pub(crate) analyzer_registry: Arc<ArcSwap<AnalyzerRegistry>>,
    pub persist: Option<PersistHandle>,
    pub(crate) filter_config: Arc<ArcSwap<crate::filter::FilterConfig>>,
    pub(crate) filter_service: Option<Arc<FilterService>>,
    pub(crate) system_replay: Arc<crate::filter::replay::ReplayRegistry>,
    pub(crate) injected_replay: Arc<crate::injected_replay::InjectedBlockRegistry>,
    pub(crate) composer: Arc<ComposerService>,
    pub(crate) embedding_service: EmbeddingService,
    pub(crate) rerank_service: crate::analysis::RerankService,
    pub(crate) semantic: crate::semantic::SemanticHandle,
    pub(crate) in_flight: Arc<crate::in_flight::InFlightRegistry>,
    pub(crate) shed_registry: Arc<crate::memory_cap::ShedRegistry>,
    pub(crate) capture_admission_throttled: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) lesson_cache: crate::lesson_worker::LessonCache,
    pub(crate) keeper_snapshot: crate::memory_keeper::snapshot::KeeperCache,
    pub(crate) change_feed: crate::change_feed::ChangeFeed,
    pub(crate) poll_answers: crate::api::poll_answers::PollAnswers,

    pub(crate) steerer_registry: Arc<ArcSwap<SteererRegistry>>,

    pub(crate) steerer_service: Option<Arc<SteeringService>>,

    pub(crate) feature_toggle_overlay: Arc<ArcSwap<FeatureToggles>>,

    pub(crate) feature_toggle_service: Option<Arc<FeatureToggleService>>,

    pub settings_overlay: Arc<ArcSwap<SettingsOverlay>>,

    pub(crate) settings_service: Option<Arc<SettingsService>>,

    pub(crate) session_toggle_cache: SessionToggleCache,

    pub(crate) channel_link_attempted: Arc<crate::channel_link_sweep::link::ChannelLinkProgress>,
    pub(crate) sessions_with_mike_tools: Arc<crate::proxy_tools::session::SessionsWithMikeTools>,

    pub(crate) injection_gate: Arc<crate::injection::InjectionGate>,

    pub(crate) workspace_registration_cells:
        Arc<dashmap::DashMap<String, Arc<tokio::sync::OnceCell<RegisteredWorkspaceSession>>>>,

    pub(crate) workspace_briefing_delivered: Arc<dashmap::DashSet<String>>,

    pub(crate) workspace_briefing_served: Arc<dashmap::DashSet<String>>,

    pub(crate) self_identity_served: Arc<dashmap::DashSet<String>>,

    pub(crate) change_record_tail_served: Arc<dashmap::DashSet<String>>,

    pub(crate) host_machine_uuid: Arc<tokio::sync::OnceCell<uuid::Uuid>>,

    pub(crate) model_catalogue: Arc<crate::steerer::model_catalogue::ModelCatalogue>,

    pub(crate) openrouter_usage_cache: Arc<crate::api::usage::OpenRouterUsageCache>,

    pub(crate) resync_status: crate::resync::ResyncStatusHandle,

    pub(crate) judge_llm: Option<crate::llm::OpenRouterClient>,

    pub(crate) llm_usage_sink: Option<Arc<dyn LlmUsageSink>>,

    pub(crate) steerer_openrouter_key: crate::llm::key::OpenRouterKeySlot,

    pub(crate) anthropic_api_key: crate::llm::key::AnthropicKeySlot,

    pub(crate) service_token: crate::service_token::ServiceToken,

    pub(crate) anthropic_cli: crate::anthropic_cli::AnthropicCliState,
}
