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

pub struct FleetArgs {
    pub rx: tokio::sync::broadcast::Receiver<ProxyEvent>,
    pub event_tx: BroadcastSender<ProxyEvent>,
    pub store: Arc<RwLock<RequestStore>>,
    pub persist: Option<PersistHandle>,
    pub steerer_registry: Arc<ArcSwap<SteererRegistry>>,
    pub llm: OpenRouterClient,
    pub key_slot: crate::llm::key::OpenRouterKeySlot,
    pub toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    pub config: SteererConfig,
    pub shutdown: CancellationToken,
    pub remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub injection_gate: Arc<crate::injection::InjectionGate>,
    pub settings: crate::settings::SharedSettingsOverlay,
}

pub struct FleetCtx {
    pub event_tx: BroadcastSender<ProxyEvent>,
    pub store: Arc<RwLock<RequestStore>>,
    pub persist: Option<PersistHandle>,
    pub steerer_registry: Arc<ArcSwap<SteererRegistry>>,
    pub llm: OpenRouterClient,
    pub key_slot: crate::llm::key::OpenRouterKeySlot,
    pub toggles: Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
    pub config: SteererConfig,
    pub shutdown: CancellationToken,
    pub remote_control_streams: crate::proxy::remote_control::ReplyStreams,
    pub injection_gate: Arc<crate::injection::InjectionGate>,
    pub settings: crate::settings::SharedSettingsOverlay,
}

impl FleetCtx {
    pub fn push_deps(&self) -> crate::proxy::remote_control::inject::PushDeps {
        crate::proxy::remote_control::inject::PushDeps {
            persist: self.persist.clone(),
            remote_control_streams: self.remote_control_streams.clone(),
            gate: self.injection_gate.clone(),
            store: self.store.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DebounceLedger {
    suppressed_topics: HashMap<String, u32>,
}

impl DebounceLedger {
    pub fn check_and_record_fire(&mut self, topic: &str, debounce_turns: u32) -> bool {
        if let Some(remaining) = self.suppressed_topics.get(topic)
            && *remaining > 0
        {
            return false;
        }
        self.suppressed_topics
            .insert(topic.to_owned(), debounce_turns);
        true
    }

    pub fn tick(&mut self) {
        self.suppressed_topics.retain(|_, remaining| {
            if *remaining == 0 {
                false
            } else {
                *remaining -= 1;
                true
            }
        });
    }

    pub fn allow_retry(&mut self) {
        self.suppressed_topics.remove("_");
    }
}

#[derive(Debug, Default)]
pub struct RequestBuffer {
    pub response_complete: bool,
    pub diagnostics: Option<Vec<Diagnostic>>,
    pub session_id: Option<String>,
    pub last_user_message: Option<String>,
    pub accumulated_text: Option<String>,
    pub stop_reason: Option<String>,
}

impl RequestBuffer {
    pub fn is_ready(&self) -> bool {
        self.response_complete && self.diagnostics.is_some()
    }
}

#[derive(Default)]
pub struct EvalInputs {
    pub eval_text: String,
    pub operator_ask: String,
    pub last_user_message: String,
    pub tool_calls_summary: String,
    pub diagnostics: Vec<Diagnostic>,
    pub anchor: Option<Anchor>,
    pub trigger_point: &'static str,
    pub turn_origin: String,
    pub subagent_persona: Option<String>,
    pub judged_turn_id: Option<i64>,
    pub evaluation_turn_id: Option<i64>,
    pub question: Option<String>,
}

pub struct EvalTarget {
    pub raw_id: u64,
    pub trigger_request_id: Option<i64>,
    pub session_id: String,
}

impl EvalTarget {
    pub fn from_raw(session_id: String, raw_id: u64) -> Self {
        Self {
            raw_id,
            trigger_request_id: Some(raw_id as i64),
            session_id,
        }
    }
}

pub struct SteererSelection<'r> {
    pub steerers: Vec<&'r Steerer>,
    pub enrolment_by_steerer_id: Option<HashMap<i64, EnrolmentRow>>,
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
