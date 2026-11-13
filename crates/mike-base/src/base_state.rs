use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use arc_swap::ArcSwap;
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::config::ProxyConfig;
use crate::feature_toggles::FeatureToggles;
use crate::in_flight::InFlightRegistry;
use crate::llm::LlmUsageSink;
use crate::llm::key::OpenRouterKeySlot;
use crate::settings::SettingsOverlay;
use crate::store::RequestStore;
use crate::store::models::{ProxyEvent, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::pool_classes::PoolClass;
use crate::store::retention::RetentionPolicy;

pub type PushFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

pub trait MailPush: Send + Sync {
    fn push_via_remote_control<'a>(
        &'a self,
        recipient: Uuid,
        reminder: &'a SystemReminder,
    ) -> PushFuture<'a>;

    fn deliver_or_enqueue(&self, recipient: Uuid, reminder: SystemReminder) -> PushFuture<'_>;
}

pub struct NoMailPush;

impl MailPush for NoMailPush {
    fn push_via_remote_control<'a>(
        &'a self,
        _recipient: Uuid,
        _reminder: &'a SystemReminder,
    ) -> PushFuture<'a> {
        Box::pin(async { false })
    }

    fn deliver_or_enqueue(&self, _recipient: Uuid, _reminder: SystemReminder) -> PushFuture<'_> {
        Box::pin(async { false })
    }
}

#[derive(Clone)]
pub struct BaseState {
    pub store: Arc<RwLock<RequestStore>>,
    pub client: reqwest::Client,
    pub event_tx: broadcast::Sender<ProxyEvent>,
    pub config: Arc<ProxyConfig>,
    pub shutdown: CancellationToken,
    pub tracker: TaskTracker,
    pub retention: RetentionPolicy,
    pub request_count: Arc<AtomicU64>,
    pub last_request_at_unix: Arc<AtomicI64>,
    pub last_capturable_request_at_unix: Arc<AtomicI64>,
    pub started_at: std::time::Instant,
    pub metrics_handle: PrometheusHandle,
    pub persist: Option<PersistHandle>,
    pub in_flight: Arc<InFlightRegistry>,
    pub change_feed: ChangeFeed,
    pub feature_toggle_overlay: Arc<ArcSwap<FeatureToggles>>,
    pub settings_overlay: Arc<ArcSwap<SettingsOverlay>>,
    pub host_machine_uuid: Arc<tokio::sync::OnceCell<Uuid>>,
    pub llm_usage_sink: Option<Arc<dyn LlmUsageSink>>,
    pub steerer_openrouter_key: OpenRouterKeySlot,
    pub mail_push: Arc<dyn MailPush>,
}

impl BaseState {
    pub fn store(&self) -> Arc<RwLock<RequestStore>> {
        self.store.clone()
    }

    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }

    pub fn shutdown(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub fn tracker(&self) -> TaskTracker {
        self.tracker.clone()
    }

    pub fn retention(&self) -> RetentionPolicy {
        self.retention.clone()
    }

    pub fn persist(&self) -> Option<&PersistHandle> {
        self.persist.as_ref()
    }

    pub fn mail_push(&self) -> &dyn MailPush {
        self.mail_push.as_ref()
    }

    pub fn on_pool_class(&self, class: PoolClass) -> Self {
        let mut state = self.clone();
        state.persist = self.persist.as_ref().map(|handle| handle.on(class));
        state
    }

    pub fn openrouter_key(&self) -> Option<secrecy::SecretString> {
        crate::llm::key::resolve(&self.steerer_openrouter_key)
    }

    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    pub fn in_flight(&self) -> Arc<InFlightRegistry> {
        self.in_flight.clone()
    }

    pub async fn in_memory_store_len(&self) -> usize {
        self.store.read().await.len()
    }

    pub fn inc_request_count(&self) -> u64 {
        self.last_request_at_unix
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
        self.request_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn last_request_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        stamp_to_time(self.last_request_at_unix.load(Ordering::Relaxed))
    }

    pub fn note_capturable_request(&self) {
        self.last_capturable_request_at_unix
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
    }

    pub fn last_capturable_request_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        stamp_to_time(self.last_capturable_request_at_unix.load(Ordering::Relaxed))
    }

    pub async fn resolve_host_machine_uuid(&self) -> Option<Uuid> {
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

    pub fn spawn_change_feed(&self) {
        crate::change_feed::spawn_change_feed(
            &self.change_feed,
            self.persist(),
            self.event_tx.subscribe(),
            self.shutdown.clone(),
            &self.tracker,
        );
    }
}

fn stamp_to_time(secs: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    match secs {
        0 => None,
        secs => chrono::DateTime::from_timestamp(secs, 0),
    }
}
