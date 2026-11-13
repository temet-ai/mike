use serde_json::Value;
use sqlx::types::Json;
use uuid::Uuid;

use super::{RuntimeWriteOutcome, invalid, same_or_conflict};
use crate::runtime::{RuntimeEventEnvelope, RuntimeObservation};
use crate::store::persistence::PersistError;

struct ObservationRow {
    envelope: Json<RuntimeEventEnvelope>,
    raw_payload: Vec<u8>,
    parsed_payload: Option<Json<Value>>,
}

impl From<ObservationRow> for RuntimeObservation {
    fn from(row: ObservationRow) -> Self {
        Self {
            envelope: row.envelope.0,
            raw_payload: row.raw_payload,
            parsed_payload: row.parsed_payload.map(|value| value.0),
        }
    }
}

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn append_runtime_observation(
        self,
        observation: &RuntimeObservation,
    ) -> Result<RuntimeWriteOutcome, PersistError> {
        observation.validate().map_err(invalid)?;
        let envelope = &observation.envelope;
        let parsed = observation
            .parsed_payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let inserted = sqlx::query_scalar!(
            "INSERT INTO runtime.events
             (namespace_id, ingestion_id, provider_thread_id, envelope, raw_payload, parsed_payload)
             VALUES ($1, $2, $3, $4::text::json, $5, $6::text::json)
             ON CONFLICT (namespace_id, ingestion_id) DO NOTHING RETURNING id",
            envelope.namespace_id,
            envelope.ingestion_id,
            envelope.provider_thread_id.as_deref(),
            serde_json::to_string(envelope)?,
            observation.raw_payload.as_slice(),
            parsed,
        )
        .fetch_optional(&self.pool)
        .await?;
        if inserted.is_some() {
            return Ok(RuntimeWriteOutcome::Inserted);
        }
        let existing = self
            .runtime_observation(envelope.namespace_id, envelope.ingestion_id)
            .await?;
        same_or_conflict(existing, observation, "observation replay")
    }

    pub async fn runtime_observation(
        self,
        namespace_id: Uuid,
        ingestion_id: Uuid,
    ) -> Result<Option<RuntimeObservation>, PersistError> {
        let row = sqlx::query_as!(
            ObservationRow,
            r#"SELECT envelope AS "envelope!: Json<RuntimeEventEnvelope>",
                      raw_payload AS "raw_payload!",
                      parsed_payload AS "parsed_payload?: Json<Value>"
                 FROM runtime.events
                WHERE namespace_id = $1 AND ingestion_id = $2"#,
            namespace_id,
            ingestion_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }
}
