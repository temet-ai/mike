use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::IngestSource;
use super::envelope::{EnvelopeInput, envelope};
use crate::runtime::codex::{
    ExclusionReason, IntakeContext, IntakeResult, RequestId, RpcScope, TrafficClass, observe,
};
use crate::runtime::{RuntimeContractError, RuntimeObservation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRoute {
    Notification,
    ServerRequest,
    Response,
    Unparsed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestFrame {
    pub observation: RuntimeObservation,
    pub rpc_scope: Option<RpcScope>,
    pub connection_epoch: Uuid,
    pub frame_ordinal: i64,
    pub route: FrameRoute,
}

impl IngestFrame {
    pub fn intake_context(&self) -> IntakeContext<'_> {
        IntakeContext {
            rpc_scope: self.rpc_scope.as_ref(),
            traffic_class: TrafficClass::OrdinaryContent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    Capture(Box<IngestFrame>),
    Excluded {
        reason: ExclusionReason,
        route: FrameRoute,
        method: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct IngestConnection {
    namespace_id: Uuid,
    source: IngestSource,
    epoch: Uuid,
    next_ordinal: i64,
    pending: HashMap<RequestId, RpcScope>,
}

impl IngestConnection {
    pub fn new(namespace_id: Uuid, source: IngestSource) -> Self {
        Self {
            namespace_id,
            source,
            epoch: Uuid::new_v4(),
            next_ordinal: 0,
            pending: HashMap::new(),
        }
    }

    pub fn epoch(&self) -> Uuid {
        self.epoch
    }

    pub fn pending_requests(&self) -> usize {
        self.pending.len()
    }

    pub fn reconnect(&mut self) -> Uuid {
        self.epoch = Uuid::new_v4();
        self.next_ordinal = 0;
        self.pending.clear();
        self.epoch
    }

    pub fn register(&mut self, scope: RpcScope) {
        self.pending.insert(scope.request_id.clone(), scope);
    }

    pub fn sent(&mut self, raw: &[u8]) -> Option<RpcScope> {
        let value = serde_json::from_slice::<Value>(raw).ok()?;
        let method = value.get("method")?.as_str()?.to_owned();
        let request_id = serde_json::from_value::<RequestId>(value.get("id")?.clone()).ok()?;
        let params = value.get("params");
        let text = |key: &str| {
            params
                .and_then(|params| params.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let scope = RpcScope {
            request_id,
            method,
            thread_id: text("threadId"),
            turn_id: text("turnId"),
        };
        self.register(scope.clone());
        Some(scope)
    }

    pub fn received(
        &mut self,
        raw: Vec<u8>,
        received_at: DateTime<Utc>,
    ) -> Result<IngestOutcome, RuntimeContractError> {
        let parsed = serde_json::from_slice::<Value>(&raw).ok();
        let route = route(parsed.as_ref());
        let method = parsed
            .as_ref()
            .and_then(|value| value.get("method"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let rpc_scope = (route == FrameRoute::Response)
            .then(|| self.take_scope(parsed.as_ref()))
            .flatten();
        let envelope = envelope(EnvelopeInput {
            namespace_id: self.namespace_id,
            source: &self.source,
            parsed: parsed.as_ref(),
            scope: rpc_scope.as_ref(),
            cursor: format!("{}:{}", self.epoch, self.next_ordinal),
            received_at,
        });
        let context = IntakeContext {
            rpc_scope: rpc_scope.as_ref(),
            traffic_class: TrafficClass::OrdinaryContent,
        };
        match observe(envelope, raw, context)? {
            IntakeResult::Excluded(reason) => Ok(IngestOutcome::Excluded {
                reason,
                route,
                method,
            }),
            IntakeResult::Captured(observation) => {
                let frame_ordinal = self.next_ordinal;
                self.next_ordinal += 1;
                Ok(IngestOutcome::Capture(Box::new(IngestFrame {
                    observation: *observation,
                    rpc_scope,
                    connection_epoch: self.epoch,
                    frame_ordinal,
                    route,
                })))
            }
        }
    }

    fn take_scope(&mut self, parsed: Option<&Value>) -> Option<RpcScope> {
        let id = parsed?.get("id")?.clone();
        let request_id = serde_json::from_value::<RequestId>(id).ok()?;
        self.pending.remove(&request_id)
    }
}

fn route(parsed: Option<&Value>) -> FrameRoute {
    let Some(value) = parsed.filter(|value| value.is_object()) else {
        return FrameRoute::Unparsed;
    };
    match (value.get("method").is_some(), value.get("id").is_some()) {
        (true, true) => FrameRoute::ServerRequest,
        (true, false) => FrameRoute::Notification,
        (false, _) => FrameRoute::Response,
    }
}
