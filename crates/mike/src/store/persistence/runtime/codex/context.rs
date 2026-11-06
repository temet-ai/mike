use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::runtime::codex::{IntakeContext, RequestId, RpcScope, TrafficClass};
use crate::store::persistence::PersistError;

use super::invariant;

pub(super) const CONTEXT_VERSION: i32 = 1;
const ADMISSION_POLICY_VERSION: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassificationProvenance {
    ReviewedNotification,
    ReviewedRpcResponse,
    ReviewedOrdinaryRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum StoredTrafficClass {
    Unknown,
    OrdinaryContent,
    Authentication,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRpcScope {
    request_id: RequestId,
    method: String,
    thread_id: Option<String>,
    turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexIntakeContext {
    context_version: i32,
    admission_policy_version: i32,
    traffic_class: StoredTrafficClass,
    rpc_scope: Option<StoredRpcScope>,
    connection_epoch: Uuid,
    frame_ordinal: i64,
    provenance: ClassificationProvenance,
}

impl CodexIntakeContext {
    pub fn new(
        context: IntakeContext<'_>,
        connection_epoch: Uuid,
        frame_ordinal: i64,
        provenance: ClassificationProvenance,
    ) -> Result<Self, PersistError> {
        let context = Self {
            context_version: CONTEXT_VERSION,
            admission_policy_version: ADMISSION_POLICY_VERSION,
            traffic_class: match context.traffic_class {
                TrafficClass::Unknown => StoredTrafficClass::Unknown,
                TrafficClass::OrdinaryContent => StoredTrafficClass::OrdinaryContent,
                TrafficClass::Authentication => StoredTrafficClass::Authentication,
            },
            rpc_scope: context.rpc_scope.map(|scope| StoredRpcScope {
                request_id: scope.request_id.clone(),
                method: scope.method.clone(),
                thread_id: scope.thread_id.clone(),
                turn_id: scope.turn_id.clone(),
            }),
            connection_epoch,
            frame_ordinal,
            provenance,
        };
        context.validate()?;
        Ok(context)
    }

    pub(super) fn validate(&self) -> Result<(), PersistError> {
        if self.context_version != CONTEXT_VERSION
            || self.admission_policy_version != ADMISSION_POLICY_VERSION
        {
            return Err(invariant(
                "unsupported intake context or admission policy version",
            ));
        }
        if self.connection_epoch.is_nil() || self.frame_ordinal < 0 {
            return Err(invariant(
                "invalid connection epoch or transport frame ordinal",
            ));
        }
        Ok(())
    }

    pub(super) fn scope(&self) -> Option<RpcScope> {
        self.rpc_scope.as_ref().map(|scope| RpcScope {
            request_id: scope.request_id.clone(),
            method: scope.method.clone(),
            thread_id: scope.thread_id.clone(),
            turn_id: scope.turn_id.clone(),
        })
    }

    pub(super) fn borrowed<'a>(&self, rpc_scope: Option<&'a RpcScope>) -> IntakeContext<'a> {
        IntakeContext {
            rpc_scope,
            traffic_class: match self.traffic_class {
                StoredTrafficClass::Unknown => TrafficClass::Unknown,
                StoredTrafficClass::OrdinaryContent => TrafficClass::OrdinaryContent,
                StoredTrafficClass::Authentication => TrafficClass::Authentication,
            },
        }
    }

    pub(super) fn connection_epoch(&self) -> Uuid {
        self.connection_epoch
    }

    pub(super) fn frame_ordinal(&self) -> i64 {
        self.frame_ordinal
    }
}
