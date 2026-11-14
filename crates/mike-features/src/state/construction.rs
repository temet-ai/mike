use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64};

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::analysis::AnalyzerRegistry;
use crate::filter::FilterConfig;
use crate::settings::SettingsOverlay;
use crate::store::RequestStore;

use super::construction_llm as llm_wiring;
use super::construction_pipeline as pipeline;
use super::construction_services as services;
use super::{AppState, BaseState};
use crate::config::ProxyConfig;

impl AppState {
    pub async fn new(config: ProxyConfig) -> anyhow::Result<Self> {
        let registry = AnalyzerRegistry::with_defaults(&config.analysis)?;
        Self::new_with_registry(config, registry).await
    }

    pub async fn new_with_shutdown(
        config: ProxyConfig,
        shutdown: CancellationToken,
    ) -> anyhow::Result<Self> {
        let registry = AnalyzerRegistry::with_defaults(&config.analysis)?;
        Self::new_with_registry_and_shutdown(config, registry, shutdown).await
    }

    pub async fn new_with_registry(
        config: ProxyConfig,
        analyzer_registry: AnalyzerRegistry,
    ) -> anyhow::Result<Self> {
        Self::new_with_registry_and_shutdown(config, analyzer_registry, CancellationToken::new())
            .await
    }

    async fn new_with_registry_and_shutdown(
        config: ProxyConfig,
        analyzer_registry: AnalyzerRegistry,
        shutdown: CancellationToken,
    ) -> anyhow::Result<Self> {
        let mut config = config;
        let retention = pipeline::build_retention_policy(&config);
        let client = pipeline::build_http_client()?;
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let mut store = RequestStore::new(retention.clone());
        let analyzer_registry = Arc::new(ArcSwap::from_pointee(analyzer_registry));
        store.set_analysis(analyzer_registry.clone(), event_tx.clone());
        let metrics_handle = pipeline::install_metrics(retention.max_store_bytes)?;
        let persist = services::seed_persistence(&config).await?;
        let tracker = TaskTracker::new();
        let llm_usage_sink = services::seed_llm_usage_sink(persist.as_ref(), &tracker).await;
        services::fold_db_settings(persist.as_ref(), &mut config).await;
        services::attach_persist_to_store(&mut store, persist.as_ref());
        let steerer_openrouter_key = llm_wiring::seed_openrouter_key_slot(&config);
        let embedding_service = pipeline::wire_embedding_service(&mut store, &config.embedding);
        let rerank_service = crate::analysis::RerankService::new(&config.rerank);
        let settings_overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
        crate::llm::provider::install(settings_overlay.clone());
        warn_retired_correction(&config);
        let judge_llm = llm_wiring::build_judge_llm(&config.steerer, llm_usage_sink.as_ref());
        let filter_config = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
        let filter_service =
            services::seed_filter_service(persist.as_ref(), &filter_config).await?;
        let (steerer_registry, steerer_service, settings_service) =
            services::seed_config(persist.as_ref(), &settings_overlay, &config).await;
        let feature_toggle_overlay = pipeline::build_feature_toggle_overlay(&config);
        let feature_toggle_service =
            services::seed_feature_toggle_service(persist.as_ref(), &feature_toggle_overlay).await;
        let composer = pipeline::seed_composer(&config, persist.as_ref()).await;
        let in_flight = pipeline::build_in_flight_registry(config.max_in_flight);
        let store_arc = Arc::new(RwLock::new(store));
        let shed_registry = pipeline::build_shed_registry(&store_arc);
        let model_catalogue = pipeline::build_model_catalogue(&client, &config.steerer);
        let (base, remote_control_streams, injection_gate) = Wired {
            store: store_arc,
            client,
            event_tx,
            config,
            shutdown,
            tracker,
            retention,
            metrics_handle,
            persist,
            in_flight,
            feature_toggle_overlay,
            settings_overlay,
            llm_usage_sink,
            steerer_openrouter_key,
        }
        .into_base();
        let state = Self {
            base,
            remote_control_streams,
            remote_control_writers: Default::default(),
            analyzer_registry,
            filter_config,
            filter_service,
            system_replay: Arc::new(crate::filter::replay::ReplayRegistry::new()),
            injected_replay: Arc::new(crate::injected_replay::InjectedBlockRegistry::default()),
            composer,
            embedding_service,
            rerank_service,
            semantic: crate::semantic::SemanticHandle::default(),
            shed_registry,
            capture_admission_throttled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            poll_answers: crate::api::poll_answers::PollAnswers::default(),
            lesson_cache: Arc::new(ArcSwap::from_pointee(
                crate::lesson_cache::LessonSnapshot::default(),
            )),
            lesson_hooks: Default::default(),
            keeper_snapshot: Arc::new(ArcSwap::from_pointee(
                crate::lesson_cache::KeeperSnapshot::default(),
            )),
            steerer_registry,
            steerer_service,
            feature_toggle_service,
            settings_service,
            injection_gate,
            session_toggle_cache: Arc::new(dashmap::DashMap::new()),
            channel_link_attempted: Arc::new(dashmap::DashMap::new()),
            sessions_with_mike_tools: Arc::new(dashmap::DashSet::new()),
            workspace_registration_cells: Arc::new(dashmap::DashMap::new()),
            workspace_briefing_delivered: Arc::new(dashmap::DashSet::new()),
            workspace_briefing_served: Arc::new(dashmap::DashSet::new()),
            self_identity_served: Arc::new(dashmap::DashSet::new()),
            change_record_tail_served: Arc::new(dashmap::DashSet::new()),
            model_catalogue,
            openrouter_usage_cache: Arc::new(crate::api::usage::OpenRouterUsageCache::new()),
            resync_status: Arc::new(ArcSwap::from_pointee(crate::resync::ResyncStatus::default())),
            judge_llm,
            anthropic_api_key: llm_wiring::seed_anthropic_key_slot(),
            service_token: crate::service_token::ServiceToken::generate(),
            anthropic_cli: crate::anthropic_cli::AnthropicCliState::new(),
        };
        Ok(finish(state).await)
    }
}

struct Wired {
    store: Arc<RwLock<RequestStore>>,
    client: reqwest::Client,
    event_tx: tokio::sync::broadcast::Sender<crate::store::models::ProxyEvent>,
    config: ProxyConfig,
    shutdown: CancellationToken,
    tracker: TaskTracker,
    retention: crate::store::retention::RetentionPolicy,
    metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
    persist: Option<crate::store::persistence::PersistHandle>,
    in_flight: Arc<crate::in_flight::InFlightRegistry>,
    feature_toggle_overlay: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    settings_overlay: Arc<ArcSwap<SettingsOverlay>>,
    llm_usage_sink: Option<Arc<dyn crate::llm::LlmUsageSink>>,
    steerer_openrouter_key: crate::llm::key::OpenRouterKeySlot,
}

impl Wired {
    fn into_base(
        self,
    ) -> (
        BaseState,
        crate::proxy::remote_control::ReplyStreams,
        Arc<crate::injection::InjectionGate>,
    ) {
        let remote_control_streams = crate::proxy::remote_control::ReplyStreams::default();
        let injection_gate = Arc::new(crate::injection::InjectionGate::default());
        let mail_push = Arc::new(super::mail_push::ProxyMailPush::new(
            crate::proxy::remote_control::inject::PushDeps {
                persist: self.persist.clone(),
                remote_control_streams: remote_control_streams.clone(),
                gate: injection_gate.clone(),
                store: self.store.clone(),
            },
        ));
        let base = BaseState {
            store: self.store,
            client: self.client,
            event_tx: self.event_tx,
            config: Arc::new(self.config),
            shutdown: self.shutdown,
            tracker: self.tracker,
            retention: self.retention,
            request_count: Arc::new(AtomicU64::new(0)),
            last_request_at_unix: Arc::new(AtomicI64::new(0)),
            last_capturable_request_at_unix: Arc::new(AtomicI64::new(0)),
            started_at: std::time::Instant::now(),
            metrics_handle: self.metrics_handle,
            persist: self.persist,
            in_flight: self.in_flight,
            change_feed: crate::change_feed::ChangeFeed::new(),
            feature_toggle_overlay: self.feature_toggle_overlay,
            settings_overlay: self.settings_overlay,
            host_machine_uuid: Arc::new(tokio::sync::OnceCell::new()),
            llm_usage_sink: self.llm_usage_sink,
            steerer_openrouter_key: self.steerer_openrouter_key,
            mail_push,
            code_notes: Arc::new(mike_base::code_notes::CodeNotes::default()),
        };
        (base, remote_control_streams, injection_gate)
    }
}

fn warn_retired_correction(config: &ProxyConfig) {
    if config.steerer.correction.enabled {
        tracing::warn!(
            "steerer.correction is set, but the correction classifier is retired; the setting is ignored"
        );
    }
}

async fn finish(state: AppState) -> AppState {
    if state.persist.is_some() {
        state.resolve_host_machine_uuid().await;
    }
    log_constructed(&state);
    state
}

fn log_constructed(state: &AppState) {
    debug_assert!(Arc::strong_count(state.analyzer_registry()) >= 2);
    tracing::debug!(
        persist_active = state.persist().is_some(),
        "AppState constructed"
    );
}
