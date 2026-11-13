use crate::store::persistence::proxy_repo::ProxyPersist as _;
use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::RwLock;

use crate::analysis::EmbeddingService;
use crate::compose_prompt::ComposerService;
use crate::compose_prompt::cache::CompositionCache;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::retention::RetentionPolicy;

use crate::config::ProxyConfig;

pub use crate::feature_toggles::build_feature_toggle_overlay;

pub fn build_retention_policy(config: &ProxyConfig) -> RetentionPolicy {
    let max_age = RetentionPolicy::parse_duration(config.max_age());
    let retention = RetentionPolicy {
        max_entries: config.max_requests(),
        max_age,
        max_store_bytes: config.max_store_bytes,
        ..RetentionPolicy::default()
    };
    tracing::info!(
        max_entries = retention.max_entries,
        max_store_bytes = retention.max_store_bytes,
        max_store_bytes_human = %bytesize::ByteSize::b(retention.max_store_bytes),
        max_age_secs = retention.max_age.as_secs(),
        prune_interval_secs = retention.prune_interval.as_secs(),
        "RequestStore retention policy configured"
    );
    retention
}

pub fn build_http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .connect_timeout(std::time::Duration::from_secs(10))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {e}"))
}

pub fn install_metrics(max_store_bytes: u64) -> anyhow::Result<PrometheusHandle> {
    let handle = crate::metrics::install_recorder()?;
    crate::proxy::remote_control::route::describe_remote_control_metrics();
    metrics::gauge!(crate::metrics::GAUGE_STORE_BYTES_BUDGET).set(max_store_bytes as f64);
    Ok(handle)
}

pub fn wire_embedding_service(
    store: &mut RequestStore,
    config: &crate::analysis::EmbeddingConfig,
) -> EmbeddingService {
    let embedding_service = EmbeddingService::new(config);
    log_embedding_pipeline_status(&embedding_service, config);
    let embedding_semaphore = Arc::new(tokio::sync::Semaphore::new(embedding_permit_count()));
    store.set_embedding(embedding_service.clone(), embedding_semaphore);
    embedding_service
}

fn log_embedding_pipeline_status(
    service: &EmbeddingService,
    config: &crate::analysis::EmbeddingConfig,
) {
    if service.is_available() {
        log_embedding_enabled(service, config);
    } else {
        log_embedding_disabled();
    }
}

fn log_embedding_enabled(service: &EmbeddingService, config: &crate::analysis::EmbeddingConfig) {
    tracing::info!(
        model = service.model_name(),
        dim = service.dim(),
        url = %config.url,
        "embedding pipeline ENABLED (HTTP/Infinity) — recall_similar_turns will use cosine ANN; FTS fallback on Infinity error"
    );
}

fn log_embedding_disabled() {
    tracing::info!(
        "embedding pipeline DISABLED via settings.enabled = false — recall_similar_turns falls back to Postgres FTS"
    );
}

fn embedding_permit_count() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(2)
        .max(1)
}

pub async fn seed_composer(
    config: &ProxyConfig,
    persist: Option<&PersistHandle>,
) -> Arc<ComposerService> {
    let composer_cache = Arc::new(CompositionCache::empty());
    let volatile_cache = Arc::new(crate::compose_prompt::volatile::VolatilePatternCache::empty());
    if config.composition_enabled {
        seed_composer_when_enabled(persist, &composer_cache, &volatile_cache).await;
    } else {
        tracing::info!(
            "prompt composer pipeline is DISABLED via --composition-enabled — every request is byte-identical passthrough"
        );
    }
    Arc::new(ComposerService::new(
        composer_cache,
        volatile_cache,
        config.composition_enabled,
    ))
}

async fn seed_composer_when_enabled(
    persist: Option<&PersistHandle>,
    composer_cache: &CompositionCache,
    volatile_cache: &crate::compose_prompt::volatile::VolatilePatternCache,
) {
    if let Some(handle) = persist {
        seed_composer_cache_and_agents(handle, composer_cache, volatile_cache).await;
    }
    tracing::info!("prompt composer pipeline is ENABLED (mode=observe is the default per class)");
}

async fn seed_composer_cache_and_agents(
    handle: &PersistHandle,
    composer_cache: &CompositionCache,
    volatile_cache: &crate::compose_prompt::volatile::VolatilePatternCache,
) {
    seed_composer_cache(handle, composer_cache).await;
    if let Err(err) = handle.proxy().reload_volatile_cache(volatile_cache).await {
        tracing::warn!(
            error = %err,
            "failed to seed volatile-line registry; continuing with empty cache"
        );
    }
    scan_file_backed_agents(handle).await;
}

async fn seed_composer_cache(handle: &PersistHandle, composer_cache: &CompositionCache) {
    let seed = match handle.proxy().load_prompt_compositions().await {
        Ok(seed) => seed,
        Err(err) => {
            log_composer_cache_seed_failed(&err);
            return;
        }
    };
    log_composer_cache_seeded(seed.len());
    composer_cache.store(seed);
}

fn log_composer_cache_seed_failed(err: &crate::store::persistence::PersistError) {
    tracing::warn!(
        error = %err,
        "failed to seed composer cache; continuing with empty cache"
    );
}

fn log_composer_cache_seeded(classes: usize) {
    tracing::info!(classes, "composer cache seeded from overrides.overrides");
}

async fn scan_file_backed_agents(handle: &PersistHandle) {
    let found = match crate::agents::scanner::scan_disk_agents(handle).await {
        Ok(found) => found,
        Err(err) => {
            log_agent_scan_failed(&err);
            return;
        }
    };
    log_agent_scan_completed(found);
}

fn log_agent_scan_failed(err: &crate::store::persistence::PersistError) {
    tracing::warn!(
        error = %err,
        "agents disk scan failed; continuing without file-backed promotion"
    );
}

fn log_agent_scan_completed(agents_scanned: usize) {
    tracing::info!(
        agents_scanned,
        "agents disk scan completed (file-backed agents promoted/registered)"
    );
}

pub fn build_in_flight_registry(
    max_in_flight: u64,
) -> Arc<crate::in_flight::InFlightRegistry> {
    let cap = usize::try_from(max_in_flight).unwrap_or(usize::MAX);
    Arc::new(crate::in_flight::InFlightRegistry::with_capacity(cap))
}

pub fn build_shed_registry(
    store_arc: &Arc<RwLock<RequestStore>>,
) -> Arc<crate::memory_cap::ShedRegistry> {
    let mut reg = crate::memory_cap::ShedRegistry::new();
    let store_handle: Arc<dyn crate::memory_cap::Sheddable> =
        Arc::new(crate::store::RequestStoreShedHandle::new(store_arc.clone()));
    reg.register(store_handle);
    Arc::new(reg)
}

pub fn build_model_catalogue(
    client: &reqwest::Client,
    config: &crate::config::steerer::SteererConfig,
) -> Arc<crate::steerer::model_catalogue::ModelCatalogue> {
    use secrecy::ExposeSecret;
    let catalogue_api_key: Option<String> = config
        .openrouter_api_key
        .as_ref()
        .map(|s| s.expose_secret().to_owned())
        .or_else(|| std::env::var("MIKE_STEERER_OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok());
    Arc::new(crate::steerer::model_catalogue::ModelCatalogue::new(
        client.clone(),
        config.openrouter_base_url.clone(),
        catalogue_api_key,
    ))
}
