use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{RuntimeContractError, validate_extensions, validate_text, validate_uuid};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEventSource {
    pub kind: String,
    pub source_version: Option<String>,
    pub adapter: String,
    pub adapter_version: String,
    pub runtime_version: Option<String>,
    pub fidelity: String,
    pub completeness: String,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl RuntimeEventSource {
    fn validate(&self) -> Result<(), RuntimeContractError> {
        for (value, field) in [
            (&self.kind, "source_kind"),
            (&self.adapter, "adapter"),
            (&self.adapter_version, "adapter_version"),
            (&self.fidelity, "fidelity"),
            (&self.completeness, "completeness"),
        ] {
            validate_text(value, field)?;
        }
        for value in [&self.source_version, &self.runtime_version]
            .into_iter()
            .flatten()
        {
            validate_text(value, "source_or_runtime_version")?;
        }
        validate_extensions(
            &self.extra,
            &[
                "kind",
                "source_version",
                "adapter",
                "adapter_version",
                "runtime_version",
                "fidelity",
                "completeness",
            ],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEventEnvelope {
    pub version: u32,
    pub ingestion_id: Uuid,
    pub namespace_id: Uuid,
    pub source: RuntimeEventSource,
    pub received_at: DateTime<Utc>,
    pub provider_timestamp: Option<DateTime<Utc>>,
    pub provider_session_id: Option<String>,
    pub provider_thread_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub provider_item_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub source_cursor: Option<String>,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl RuntimeEventEnvelope {
    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        if self.version == 0 {
            return Err(RuntimeContractError::InvalidEnvelopeVersion);
        }
        validate_uuid(self.ingestion_id, "ingestion_id")?;
        validate_uuid(self.namespace_id, "namespace_id")?;
        self.source.validate()?;
        for value in [
            &self.provider_session_id,
            &self.provider_thread_id,
            &self.provider_turn_id,
            &self.provider_item_id,
            &self.provider_event_id,
            &self.source_cursor,
        ]
        .into_iter()
        .flatten()
        {
            validate_text(value, "provider_id_or_cursor")?;
        }
        validate_extensions(
            &self.extra,
            &[
                "version",
                "ingestion_id",
                "namespace_id",
                "source",
                "received_at",
                "provider_timestamp",
                "provider_session_id",
                "provider_thread_id",
                "provider_turn_id",
                "provider_item_id",
                "provider_event_id",
                "source_cursor",
            ],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeObservation {
    pub envelope: RuntimeEventEnvelope,
    pub raw_payload: Vec<u8>,
    pub parsed_payload: Option<Value>,
}

impl RuntimeObservation {
    pub fn new(
        envelope: RuntimeEventEnvelope,
        raw_payload: Vec<u8>,
    ) -> Result<Self, RuntimeContractError> {
        envelope.validate()?;
        let parsed_payload = serde_json::from_slice(&raw_payload).ok();
        Ok(Self {
            envelope,
            raw_payload,
            parsed_payload,
        })
    }

    pub fn validate(&self) -> Result<(), RuntimeContractError> {
        self.envelope.validate()?;
        let parsed: Option<Value> = serde_json::from_slice(&self.raw_payload).ok();
        if parsed != self.parsed_payload {
            return Err(RuntimeContractError::InconsistentProjection);
        }
        Ok(())
    }
}
