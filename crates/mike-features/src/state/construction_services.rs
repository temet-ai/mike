use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;
use tokio_util::task::TaskTracker;

use crate::eval::bundles::{BundlePublisher, BundleResolver, BundleStaticInputs};
use crate::feature_toggles::FeatureToggles;
use crate::feature_toggles::service::FeatureToggleService;
use crate::filter::FilterConfig;
use crate::filter::service::FilterService;
use crate::llm::LlmUsageSink;
use crate::settings::SettingsOverlay;
use crate::settings::service::SettingsService;
use crate::steerer::registry::SteererRegistry;
use crate::steerer::service::SteeringService;
use crate::store::RequestStore;
use crate::store::persistence::{PersistError, PersistHandle};

use crate::config::ProxyConfig;
use crate::config::apply_settings_overrides;
use crate::config::persist_bootstrap::PersistLlmUsageSink;
use crate::config::write_guards::{
    FeatureToggleWriteGuard, FilterWriteGuard, SettingsWriteGuard, SteeringWriteGuard,
};

pub async fn seed_persistence(
    config: &ProxyConfig,
) -> anyhow::Result<Option<PersistHandle>> {
    let persist = crate::config::persist_bootstrap::build_persist_handle(
        config,
        crate::store::persistence::SchemaPolicy::Migrate,
    )
    .await?;
    if let Some(persist) = &persist {
        persist.set_new_session_hook(std::sync::Arc::new(
            crate::steerer::enrolment_hook::AutoSteererEnrolment,
        ));
    }
    log_persistence_mode(persist.is_some());
    Ok(persist)
}

fn log_persistence_mode(enabled: bool) {
    let mode = if enabled {
        "session persistence is enabled — Postgres pool ready"
    } else {
        "session persistence is disabled (MIKE_PERSIST_ENABLED=0) — running in-memory only"
    };
    tracing::info!("{mode}");
}

pub async fn seed_llm_usage_sink(
    persist: Option<&PersistHandle>,
    tracker: &TaskTracker,
) -> Option<Arc<dyn LlmUsageSink>> {
    let handle = persist?;
    Some(Arc::new(
        PersistLlmUsageSink::new(handle.background(), tracker.clone()).await,
    ))
}

pub fn attach_persist_to_store(store: &mut RequestStore, persist: Option<&PersistHandle>) {
    if let Some(handle) = persist {
        store.set_persist(handle.capture());
    }
}

pub async fn fold_db_settings(persist: Option<&PersistHandle>, config: &mut ProxyConfig) {
    let Some(handle) = persist else {
        return;
    };
    let rows = handle.fetch_settings().await;
    apply_fetched_settings(config, rows);
}

fn apply_fetched_settings(
    config: &mut ProxyConfig,
    rows: Result<Vec<(String, serde_json::Value)>, PersistError>,
) {
    let rows = match rows {
        Ok(rows) => rows,
        Err(err) => {
            log_settings_fetch_failed(&err);
            return;
        }
    };
    apply_settings_overrides(config, &rows);
    log_settings_folded(rows.len());
}

fn log_settings_fetch_failed(err: &PersistError) {
    tracing::warn!(
        error = %err,
        "settings: failed to fetch DB overrides; ProxyConfig stays at compiled/TOML defaults"
    );
}

fn log_settings_folded(count: usize) {
    tracing::info!(
        count,
        "settings: DB overrides folded onto ProxyConfig (OnRestart keys)"
    );
}

pub async fn seed_filter_service(
    persist: Option<&PersistHandle>,
    filter_config: &Arc<ArcSwap<FilterConfig>>,
) -> anyhow::Result<Option<Arc<FilterService>>> {
    let Some(handle) = persist else {
        return Ok(None);
    };
    let write_lock = Arc::new(Mutex::new(FilterWriteGuard));
    Ok(Some(
        FilterService::reconcile_on_startup(filter_config.clone(), write_lock, handle.clone())
            .await?,
    ))
}

pub type SeededConfig = (
    Arc<ArcSwap<SteererRegistry>>,
    Option<Arc<SteeringService>>,
    Option<Arc<SettingsService>>,
);

fn bundle_publisher(
    persist: Option<&PersistHandle>,
    settings_overlay: &Arc<ArcSwap<SettingsOverlay>>,
    steerer_registry: &Arc<ArcSwap<SteererRegistry>>,
    config: &ProxyConfig,
    write_lock: &Arc<Mutex<SettingsWriteGuard>>,
) -> Option<Arc<BundlePublisher>> {
    let handle = persist?;
    let inputs = BundleStaticInputs {
        judge: config.steerer.judge.clone(),
        session_judge: config.session_judge.clone(),
        fleet_default_model: config.steerer.model.clone(),
        smoke_run: config.smoke_run,
    };
    let resolver = BundleResolver::new(handle.clone(), inputs, steerer_registry.clone());
    Some(Arc::new(BundlePublisher::new(
        resolver,
        settings_overlay.clone(),
        write_lock.clone(),
    )))
}

pub async fn seed_config(
    persist: Option<&PersistHandle>,
    settings_overlay: &Arc<ArcSwap<SettingsOverlay>>,
    config: &ProxyConfig,
) -> SeededConfig {
    let steerer_registry = Arc::new(ArcSwap::from_pointee(SteererRegistry::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let publisher = bundle_publisher(
        persist,
        settings_overlay,
        &steerer_registry,
        config,
        &write_lock,
    );
    let steerer_service = seed_steerer_service(persist, &steerer_registry, publisher.clone()).await;
    let settings_service =
        seed_settings_service(persist, settings_overlay, write_lock, publisher).await;
    (steerer_registry, steerer_service, settings_service)
}

async fn seed_steerer_service(
    persist: Option<&PersistHandle>,
    steerer_registry: &Arc<ArcSwap<SteererRegistry>>,
    publisher: Option<Arc<BundlePublisher>>,
) -> Option<Arc<SteeringService>> {
    let handle = persist?;
    let write_lock = Arc::new(Mutex::new(SteeringWriteGuard));
    let result = SteeringService::reconcile_on_startup(
        steerer_registry.clone(),
        write_lock,
        handle.clone(),
        publisher,
    )
    .await;
    log_seed_outcome(
        result,
        "steerer registry seeded from steering.steerers",
        "failed to seed steerer registry; continuing with empty registry",
    )
}

pub async fn seed_feature_toggle_service(
    persist: Option<&PersistHandle>,
    feature_toggle_overlay: &Arc<ArcSwap<FeatureToggles>>,
) -> Option<Arc<FeatureToggleService>> {
    let handle = persist?;
    let write_lock = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let result = FeatureToggleService::reconcile_on_startup(
        feature_toggle_overlay.clone(),
        write_lock,
        handle.clone(),
    )
    .await;
    log_seed_outcome(
        result,
        "feature toggles seeded from config.feature_toggles",
        "failed to seed feature toggles; continuing with all-on defaults",
    )
}

async fn seed_settings_service(
    persist: Option<&PersistHandle>,
    settings_overlay: &Arc<ArcSwap<SettingsOverlay>>,
    write_lock: Arc<Mutex<SettingsWriteGuard>>,
    publisher: Option<Arc<BundlePublisher>>,
) -> Option<Arc<SettingsService>> {
    let handle = persist?;
    let result = SettingsService::reconcile_on_startup_with_bundles(
        settings_overlay.clone(),
        write_lock,
        handle.clone(),
        publisher,
    )
    .await;
    log_seed_outcome(
        result,
        "settings overlay seeded from config.settings",
        "failed to seed settings overlay; continuing with compiled defaults",
    )
}

fn log_seed_outcome<T>(
    result: anyhow::Result<Arc<T>>,
    ok_msg: &str,
    err_msg: &str,
) -> Option<Arc<T>> {
    let svc = match result {
        Ok(svc) => svc,
        Err(err) => {
            log_seed_failed(err_msg, &err);
            return None;
        }
    };
    log_seed_succeeded(ok_msg);
    Some(svc)
}

fn log_seed_succeeded(msg: &str) {
    tracing::info!("{msg}");
}

fn log_seed_failed(msg: &str, err: &anyhow::Error) {
    tracing::warn!(error = %err, "{msg}");
}
