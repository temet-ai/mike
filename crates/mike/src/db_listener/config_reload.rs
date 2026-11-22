use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::sync::{Notify, RwLock};
use tokio_util::sync::CancellationToken;

use crate::analysis::AnalyzerRegistry;
use crate::compose_prompt::ComposerService;
use crate::config::live::LiveConfig;
use crate::feature_toggles::service::FeatureToggleService;
use crate::filter::service::FilterService;
use crate::in_flight::InFlightRegistry;
use crate::settings::service::SettingsService;
use crate::state::AppState;
use crate::steerer::service::SteeringService;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;

pub const CONFIG_CHANNEL: &str = "mike.config_changed";

const COALESCE_WINDOW: Duration = Duration::from_millis(250);

#[derive(Clone, Default)]
pub struct ConfigReloader {
    signal: Arc<Notify>,
}

impl ConfigReloader {
    pub fn request(&self) {
        self.signal.notify_one();
    }
}

struct ReloadTargets {
    toggles: Option<Arc<FeatureToggleService>>,
    settings: Option<Arc<SettingsService>>,
    live_config: Arc<LiveConfig>,
    in_flight: Arc<InFlightRegistry>,
    store: Arc<RwLock<RequestStore>>,
    analyzer_registry: Arc<ArcSwap<AnalyzerRegistry>>,
    composer: Arc<ComposerService>,
    persist: Option<PersistHandle>,
    steerers: Option<Arc<SteeringService>>,
    filters: Option<Arc<FilterService>>,
}

pub fn spawn_config_reloader(state: &AppState) -> ConfigReloader {
    let reloader = ConfigReloader::default();
    let targets = ReloadTargets {
        toggles: state.feature_toggle_service().cloned(),
        settings: state.settings_service().cloned(),
        live_config: state.live_config.clone(),
        in_flight: state.in_flight.clone(),
        store: state.store.clone(),
        analyzer_registry: state.analyzer_registry.clone(),
        composer: state.composer.clone(),
        persist: state.persist.clone(),
        steerers: state.steerer_service().cloned(),
        filters: state.filter_service().cloned(),
    };
    let signal = reloader.signal.clone();
    let shutdown = state.shutdown.clone();
    state.tracker.spawn(reload_loop(signal, targets, shutdown));
    reloader
}

async fn reload_loop(signal: Arc<Notify>, targets: ReloadTargets, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = signal.notified() => {}
        }
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(COALESCE_WINDOW) => {}
        }
        reload_once(&targets).await;
    }
}

async fn reload_once(targets: &ReloadTargets) {
    if let Some(persist) = &targets.persist {
        persist.settings_changed();
    }
    reload_toggles(targets.toggles.as_deref()).await;
    reload_settings(targets.settings.as_deref()).await;
    reload_steerers(targets.steerers.as_deref()).await;
    reload_filters(targets.filters.as_deref()).await;
    apply_live_config(targets).await;
    log_reloaded();
}

async fn reload_steerers(steerers: Option<&SteeringService>) {
    let Some(steerers) = steerers else {
        return;
    };
    if let Err(error) = steerers.reload_from_db().await {
        tracing::warn!(%error, "db_listener: steerer registry reload failed");
    }
}

async fn reload_filters(filters: Option<&FilterService>) {
    let Some(filters) = filters else {
        return;
    };
    if let Err(error) = filters.reload_from_db().await {
        tracing::warn!(%error, "db_listener: filter rules reload failed");
    }
}

async fn reload_toggles(toggles: Option<&FeatureToggleService>) {
    let Some(toggles) = toggles else {
        return;
    };
    if let Err(error) = toggles.reload_from_db().await {
        tracing::warn!(%error, "db_listener: feature toggle reload failed");
    }
}

async fn reload_settings(settings: Option<&SettingsService>) {
    let Some(settings) = settings else {
        return;
    };
    if let Err(error) = settings.reload_from_db().await {
        tracing::warn!(%error, "db_listener: settings reload failed");
    }
}

fn log_reloaded() {
    tracing::debug!("db_listener: feature toggles and settings reloaded from the database");
}

async fn apply_live_config(targets: &ReloadTargets) {
    let config = targets.live_config.load();
    targets
        .in_flight
        .set_max(usize::try_from(config.max_in_flight).unwrap_or(usize::MAX));
    targets
        .store
        .write()
        .await
        .set_max_store_bytes(config.max_store_bytes);
    rebuild_analyzers(&targets.analyzer_registry, &config);
    crate::state::construction_pipeline::apply_composition_enabled(
        &targets.composer,
        targets.persist.as_ref(),
        config.composition_enabled,
    )
    .await;
    crate::serve::tracing_init::set_log_level(&config.log_level);
}

fn rebuild_analyzers(registry: &ArcSwap<AnalyzerRegistry>, config: &crate::config::ProxyConfig) {
    match AnalyzerRegistry::with_defaults(&config.analysis) {
        Ok(rebuilt) => registry.store(Arc::new(rebuilt)),
        Err(error) => tracing::warn!(%error, "db_listener: analyzer registry rebuild failed"),
    }
}
