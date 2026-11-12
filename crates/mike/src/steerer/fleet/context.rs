use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio_util::sync::CancellationToken;

use crate::config::steerer::SteererConfig;
use crate::llm::OpenRouterClient;
use crate::steerer::anchor::Anchor;
use crate::steerer::registry::{Steerer, SteererRegistry};
use crate::store::RequestStore;
use crate::store::models::{Diagnostic, ProxyEvent};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::EnrolmentRow;

pub(crate) struct FleetArgs {
    pub(crate) rx: tokio::sync::broadcast::Receiver<ProxyEvent>,
    pub(crate) event_tx: BroadcastSender<ProxyEvent>,
    pub(crate) store: Arc<RwLock<RequestStore>>,
    pub(crate) persist: Option<PersistHandle>,
    pub(crate) steerer_registry: Arc<ArcSwap<SteererRegistry>>,
    pub(crate) llm: OpenRouterClient,
    pub(crate) key_slot: crate::llm::key::OpenRouterKeySlot,
    pub(crate) toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    pub(crate) config: SteererConfig,
    pub(crate) shutdown: CancellationToken,
    pub(crate) remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub(crate) injection_gate: Arc<crate::injection::InjectionGate>,
    pub(crate) settings: crate::settings::SharedSettingsOverlay,
}

pub(super) struct FleetCtx {
    pub(super) event_tx: BroadcastSender<ProxyEvent>,
    pub(super) store: Arc<RwLock<RequestStore>>,
    pub(super) persist: Option<PersistHandle>,
    pub(super) steerer_registry: Arc<ArcSwap<SteererRegistry>>,
    pub(super) llm: OpenRouterClient,
    pub(super) key_slot: crate::llm::key::OpenRouterKeySlot,
    pub(super) toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    pub(super) config: SteererConfig,
    pub(super) shutdown: CancellationToken,
    pub(super) remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub(super) injection_gate: Arc<crate::injection::InjectionGate>,
    pub(super) settings: crate::settings::SharedSettingsOverlay,
}

impl FleetCtx {
    pub(super) fn push_deps(&self) -> crate::proxy::remote_control::inject::PushDeps {
        crate::proxy::remote_control::inject::PushDeps {
            persist: self.persist.clone(),
            remote_control_streams: self.remote_control_streams.clone(),
            gate: self.injection_gate.clone(),
            store: self.store.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct DebounceLedger {
    suppressed_topics: HashMap<String, u32>,
}

impl DebounceLedger {
    pub(super) fn check_and_record_fire(&mut self, topic: &str, debounce_turns: u32) -> bool {
        if let Some(remaining) = self.suppressed_topics.get(topic)
            && *remaining > 0
        {
            return false;
        }
        self.suppressed_topics
            .insert(topic.to_owned(), debounce_turns);
        true
    }

    pub(super) fn tick(&mut self) {
        self.suppressed_topics.retain(|_, remaining| {
            if *remaining == 0 {
                false
            } else {
                *remaining -= 1;
                true
            }
        });
    }

    pub(super) fn allow_retry(&mut self) {
        self.suppressed_topics.remove("_");
    }
}

#[derive(Debug, Default)]
pub(super) struct RequestBuffer {
    pub(super) response_complete: bool,
    pub(super) diagnostics: Option<Vec<Diagnostic>>,
    pub(super) session_id: Option<String>,
    pub(super) last_user_message: Option<String>,
    pub(super) accumulated_text: Option<String>,
    pub(super) stop_reason: Option<String>,
}

impl RequestBuffer {
    pub(super) fn is_ready(&self) -> bool {
        self.response_complete && self.diagnostics.is_some()
    }
}

#[derive(Default)]
pub(super) struct EvalInputs {
    pub(super) eval_text: String,
    pub(super) operator_ask: String,
    pub(super) last_user_message: String,
    pub(super) tool_calls_summary: String,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) anchor: Option<Anchor>,
    pub(super) trigger_point: &'static str,
    pub(super) turn_origin: String,
    pub(super) subagent_persona: Option<String>,
    pub(super) judged_turn_id: Option<i64>,
    pub(super) evaluation_turn_id: Option<i64>,
    pub(super) question: Option<String>,
}

pub(super) struct EvalTarget {
    pub(super) raw_id: u64,
    pub(super) trigger_request_id: Option<i64>,
    pub(super) session_id: String,
}

impl EvalTarget {
    pub(super) fn from_raw(session_id: String, raw_id: u64) -> Self {
        Self {
            raw_id,
            trigger_request_id: Some(raw_id as i64),
            session_id,
        }
    }
}

pub(super) struct SteererSelection<'r> {
    pub(super) steerers: Vec<&'r Steerer>,
    pub(super) enrolment_by_steerer_id: Option<HashMap<i64, EnrolmentRow>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_ledger_suppresses_repeat_fires() {
        let mut ledger = DebounceLedger::default();
        assert!(ledger.check_and_record_fire("off-topic-X", 3));
        assert!(!ledger.check_and_record_fire("off-topic-X", 3));
        assert!(ledger.check_and_record_fire("different-topic", 3));
    }

    #[test]
    fn debounce_ledger_tick_counts_down() {
        let mut ledger = DebounceLedger::default();
        assert!(ledger.check_and_record_fire("topic-A", 2));
        assert!(!ledger.check_and_record_fire("topic-A", 2));

        ledger.tick();
        assert!(!ledger.check_and_record_fire("topic-A", 2));

        ledger.tick();
        ledger.tick();
        assert!(ledger.check_and_record_fire("topic-A", 2));
    }

    #[test]
    fn debounce_ledger_evicts_exhausted_entries() {
        let mut ledger = DebounceLedger::default();
        assert!(ledger.check_and_record_fire("topic-A", 1));
        ledger.tick();
        ledger.tick();
        assert!(ledger.suppressed_topics.is_empty());
    }

    #[test]
    fn request_buffer_ready_when_both_present() {
        let mut buf = RequestBuffer::default();
        assert!(!buf.is_ready());
        buf.response_complete = true;
        assert!(!buf.is_ready());
        buf.diagnostics = Some(vec![]);
        assert!(buf.is_ready());
    }
}
