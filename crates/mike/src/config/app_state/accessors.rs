use std::sync::Arc;
use std::sync::atomic::Ordering;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use crate::feature_toggles::ResolvedToggles;
use crate::feature_toggles::service::FeatureToggleService;
use crate::filter::service::FilterService;
use crate::settings::service::SettingsService;
use crate::steerer::service::SteeringService;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::pool_classes::PoolClass;
use crate::store::retention::RetentionPolicy;
use tokio::sync::RwLock;

use super::super::schema::ProxyConfig;
use super::AppState;

impl AppState {
    pub fn store(&self) -> Arc<RwLock<RequestStore>> {
        self.store.clone()
    }

    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }

    pub fn shutdown(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub fn service_token(&self) -> &crate::service_token::ServiceToken {
        &self.service_token
    }

    pub fn tracker(&self) -> TaskTracker {
        self.tracker.clone()
    }

    pub fn retention(&self) -> RetentionPolicy {
        self.retention.clone()
    }

    pub(crate) fn analyzer_registry(&self) -> &Arc<ArcSwap<AnalyzerRegistry>> {
        &self.analyzer_registry
    }

    pub fn persist(&self) -> Option<&PersistHandle> {
        self.persist.as_ref()
    }

    pub(crate) fn on_pool_class(&self, class: PoolClass) -> Self {
        let mut state = self.clone();
        state.persist = self.persist.as_ref().map(|handle| handle.on(class));
        state
    }

    pub(crate) fn push_deps(&self) -> crate::proxy::remote_control::inject::PushDeps {
        crate::proxy::remote_control::inject::PushDeps {
            persist: self.persist.clone(),
            remote_control_streams: self.remote_control_streams.clone(),
            gate: self.injection_gate.clone(),
            store: self.store.clone(),
        }
    }

    #[doc(hidden)]
    pub async fn sweep_expired_sessions_for_tests(
        &self,
        default_ttl: std::time::Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<Result<u64, String>> {
        let persist = self.persist.as_ref()?;
        Some(
            persist
                .sweep_expired_sessions(default_ttl, now)
                .await
                .map_err(|e| e.to_string()),
        )
    }

    pub async fn in_memory_store_len(&self) -> usize {
        self.store.read().await.len()
    }

    pub fn injectable_lessons(
        &self,
        project_path: &str,
    ) -> Vec<crate::lesson_worker::InjectableLesson> {
        self.lesson_cache.load().for_project(project_path).to_vec()
    }

    pub fn spawn_db_pool_gauge_updater(&self) -> Option<tokio::task::JoinHandle<()>> {
        self.persist.as_ref().map(|p| {
            let pools = p
                .class_pools()
                .into_iter()
                .map(|(class, pool)| (class, pool.clone()))
                .collect();
            crate::metrics::spawn_db_pool_gauge_updater(pools, self.shutdown())
        })
    }

    pub(crate) fn filter_service(&self) -> Option<&Arc<FilterService>> {
        self.filter_service.as_ref()
    }

    pub(crate) fn steerer_service(&self) -> Option<&Arc<SteeringService>> {
        self.steerer_service.as_ref()
    }

    pub(crate) fn feature_toggle_service(&self) -> Option<&Arc<FeatureToggleService>> {
        self.feature_toggle_service.as_ref()
    }

    pub(crate) fn settings_service(&self) -> Option<&Arc<SettingsService>> {
        self.settings_service.as_ref()
    }

    pub(crate) fn model_catalogue(&self) -> &crate::steerer::model_catalogue::ModelCatalogue {
        &self.model_catalogue
    }

    pub(crate) fn openrouter_usage_cache(&self) -> &crate::api::usage::OpenRouterUsageCache {
        &self.openrouter_usage_cache
    }

    pub(crate) fn openrouter_key(&self) -> Option<secrecy::SecretString> {
        crate::llm::key::resolve(&self.steerer_openrouter_key)
    }

    pub fn resync_status(&self) -> &crate::resync::ResyncStatusHandle {
        &self.resync_status
    }

    pub(crate) fn resolve_session_toggles(&self, session_id: &str) -> Arc<ResolvedToggles> {
        self.session_toggle_cache
            .entry(session_id.to_owned())
            .or_insert_with(|| {
                let overlay = self.feature_toggle_overlay.load();
                let settings = self.settings_overlay.load();
                Arc::new(ResolvedToggles::from_overlay(&overlay, &settings))
            })
            .clone()
    }

    pub fn embedding_service(&self) -> EmbeddingService {
        self.embedding_service.clone()
    }

    pub fn semantic(&self) -> crate::semantic::SemanticHandle {
        self.semantic.clone()
    }

    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    pub fn in_flight(&self) -> Arc<crate::in_flight::InFlightRegistry> {
        self.in_flight.clone()
    }

    pub(crate) fn shed_registry(&self) -> &crate::memory_cap::ShedRegistry {
        &self.shed_registry
    }

    pub(crate) fn is_capture_admission_throttled(&self) -> bool {
        self.capture_admission_throttled
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn set_capture_admission_throttled(&self, value: bool) {
        self.capture_admission_throttled
            .store(value, std::sync::atomic::Ordering::Release);
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn set_capture_admission_throttled_for_tests(&self, value: bool) {
        self.set_capture_admission_throttled(value);
    }

    pub async fn is_session_composable(&self, session_id: &str) -> bool {
        let guard = self.store.read().await;
        !guard.is_first_composable_request_in_session(session_id)
    }

    pub(crate) fn inc_request_count(&self) -> u64 {
        self.last_request_at_unix
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
        self.request_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[cfg(test)]
    pub(crate) fn last_request_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        stamp_to_time(self.last_request_at_unix.load(Ordering::Relaxed))
    }

    pub(crate) fn note_capturable_request(&self) {
        self.last_capturable_request_at_unix
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
    }

    pub(crate) fn last_capturable_request_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        stamp_to_time(self.last_capturable_request_at_unix.load(Ordering::Relaxed))
    }

    pub(crate) async fn resolve_host_machine_uuid(&self) -> Option<uuid::Uuid> {
        self.host_machine_uuid
            .get_or_try_init(|| async {
                crate::machine_identity::load_default()
                    .await
                    .map(|identity| identity.machine_uuid)
            })
            .await
            .inspect_err(|error| tracing::debug!(%error, "proxy host has no machine identity"))
            .ok()
            .copied()
    }
}

fn stamp_to_time(secs: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    match secs {
        0 => None,
        secs => chrono::DateTime::from_timestamp(secs, 0),
    }
}
