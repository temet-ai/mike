use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use crate::feature_toggles::ResolvedToggles;
use crate::feature_toggles::service::FeatureToggleService;
use crate::filter::service::FilterService;
use crate::settings::service::SettingsService;
use crate::steerer::service::SteeringService;
use crate::store::persistence::pool_classes::PoolClass;

use super::AppState;

impl AppState {
    pub fn service_token(&self) -> &crate::service_token::ServiceToken {
        &self.service_token
    }

    pub fn analyzer_registry(&self) -> &Arc<ArcSwap<AnalyzerRegistry>> {
        &self.analyzer_registry
    }

    pub fn on_pool_class(&self, class: PoolClass) -> Self {
        let mut state = self.clone();
        state.base = self.base.on_pool_class(class);
        state
    }

    pub fn push_deps(&self) -> crate::proxy::remote_control::inject::PushDeps {
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

    pub fn injectable_lessons(
        &self,
        project_path: &str,
    ) -> Vec<crate::lesson_cache::InjectableLesson> {
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

    pub fn filter_service(&self) -> Option<&Arc<FilterService>> {
        self.filter_service.as_ref()
    }

    pub fn steerer_service(&self) -> Option<&Arc<SteeringService>> {
        self.steerer_service.as_ref()
    }

    pub fn feature_toggle_service(&self) -> Option<&Arc<FeatureToggleService>> {
        self.feature_toggle_service.as_ref()
    }

    pub fn settings_service(&self) -> Option<&Arc<SettingsService>> {
        self.settings_service.as_ref()
    }

    pub fn model_catalogue(&self) -> &crate::steerer::model_catalogue::ModelCatalogue {
        &self.model_catalogue
    }

    pub fn openrouter_usage_cache(&self) -> &crate::api::usage::OpenRouterUsageCache {
        &self.openrouter_usage_cache
    }

    pub fn resync_status(&self) -> &crate::resync::ResyncStatusHandle {
        &self.resync_status
    }

    pub fn resolve_session_toggles(&self, session_id: &str) -> Arc<ResolvedToggles> {
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

    pub fn rerank_service(&self) -> crate::analysis::RerankService {
        self.rerank_service.clone()
    }

    pub fn semantic(&self) -> crate::semantic::SemanticHandle {
        self.semantic.clone()
    }

    pub fn shed_registry(&self) -> &crate::memory_cap::ShedRegistry {
        &self.shed_registry
    }

    pub fn is_capture_admission_throttled(&self) -> bool {
        self.capture_admission_throttled
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn set_capture_admission_throttled(&self, value: bool) {
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
}
