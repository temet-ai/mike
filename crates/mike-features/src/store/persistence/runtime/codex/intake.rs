use sqlx::PgConnection;

use crate::runtime::RuntimeObservation;
use crate::runtime::codex::{IntakeResult, ReceiptId, observe};
use crate::store::persistence::PersistError;

use super::super::{RuntimeWriteOutcome, invalid};
use super::context::CONTEXT_VERSION;
use super::reads::receipt_by_ingestion;
use super::{
    CodexCaptureOutcome, CodexIntakeContext, CodexReceiptInput, StoredRuntimeReceipt, invariant,
};

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn capture_codex_receipt(
        self,
        input: CodexReceiptInput,
    ) -> Result<CodexCaptureOutcome, PersistError> {
        let scope = input.context.scope();
        let observation = match observe(
            input.envelope,
            input.raw_payload,
            input.context.borrowed(scope.as_ref()),
        )
        .map_err(invalid)?
        {
            IntakeResult::Excluded(reason) => return Ok(CodexCaptureOutcome::Excluded(reason)),
            IntakeResult::Captured(observation) => observation,
        };
        input.context.validate()?;
        let mut transaction = self.pool.begin().await?;
        lock_intake(&mut transaction, observation.envelope.namespace_id).await?;
        if let Some(existing) = existing_receipt(&mut transaction, &observation).await? {
            exact_retry(&existing, &observation, &input.context)?;
            transaction.commit().await?;
            return Ok(CodexCaptureOutcome::Captured {
                receipt: Box::new(existing),
                outcome: RuntimeWriteOutcome::Unchanged,
            });
        }
        let transport_gap = transport_order(&mut transaction, &observation, &input.context).await?;
        let event_id = insert_observation(&mut transaction, &observation).await?;
        let capture_sequence = insert_context(
            &mut transaction,
            event_id,
            &observation,
            &input.context,
            transport_gap,
        )
        .await?;
        transaction.commit().await?;
        Ok(CodexCaptureOutcome::Captured {
            receipt: Box::new(StoredRuntimeReceipt {
                event_id,
                observation: *observation,
                capture_sequence: Some(capture_sequence),
                context: Some(input.context),
                transport_gap: Some(transport_gap),
            }),
            outcome: RuntimeWriteOutcome::Inserted,
        })
    }
}

pub async fn lock_intake(
    connection: &mut PgConnection,
    namespace_id: uuid::Uuid,
) -> Result<i64, PersistError> {
    sqlx::query!(
        "INSERT INTO runtime.codex_intake_state (namespace_id)
         SELECT id FROM runtime.namespaces WHERE id = $1 AND runtime_kind = 'codex'
         ON CONFLICT (namespace_id) DO NOTHING",
        namespace_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query_scalar!(
        "SELECT last_capture_sequence FROM runtime.codex_intake_state
         WHERE namespace_id = $1 FOR UPDATE",
        namespace_id,
    )
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invariant("missing Codex namespace"))
}

async fn existing_receipt(
    connection: &mut PgConnection,
    observation: &RuntimeObservation,
) -> Result<Option<StoredRuntimeReceipt>, PersistError> {
    receipt_by_ingestion(
        connection,
        ReceiptId {
            namespace_id: observation.envelope.namespace_id,
            ingestion_id: observation.envelope.ingestion_id,
        },
    )
    .await
}

fn exact_retry(
    existing: &StoredRuntimeReceipt,
    observation: &RuntimeObservation,
    context: &CodexIntakeContext,
) -> Result<(), PersistError> {
    if existing.observation != *observation || existing.context.as_ref() != Some(context) {
        return Err(invariant(
            "receipt replay changed raw evidence or trusted context; legacy receipts cannot be reclassified",
        ));
    }
    Ok(())
}

async fn transport_order(
    connection: &mut PgConnection,
    observation: &RuntimeObservation,
    context: &CodexIntakeContext,
) -> Result<bool, PersistError> {
    let previous = sqlx::query_scalar!(
        "SELECT frame_ordinal FROM runtime.codex_receipt_context
         WHERE namespace_id = $1 AND connection_epoch = $2
         ORDER BY frame_ordinal DESC LIMIT 1",
        observation.envelope.namespace_id,
        context.connection_epoch(),
    )
    .fetch_optional(connection)
    .await?;
    if previous.is_some_and(|previous| previous >= context.frame_ordinal()) {
        return Err(invariant("transport frame ordinal duplicated or regressed"));
    }
    Ok(previous.is_some_and(|previous| previous.checked_add(1) != Some(context.frame_ordinal())))
}

pub async fn insert_observation(
    connection: &mut PgConnection,
    observation: &RuntimeObservation,
) -> Result<i64, PersistError> {
    let envelope = &observation.envelope;
    let parsed = observation
        .parsed_payload
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    sqlx::query_scalar!(
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
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invariant("concurrent legacy receipt cannot receive trusted context"))
}

pub async fn insert_context(
    connection: &mut PgConnection,
    event_id: i64,
    observation: &RuntimeObservation,
    context: &CodexIntakeContext,
    transport_gap: bool,
) -> Result<i64, PersistError> {
    let document = serde_json::to_string(context)?;
    Ok(sqlx::query_scalar!(
        r#"WITH next AS (
             UPDATE runtime.codex_intake_state
                SET last_capture_sequence = last_capture_sequence + 1
              WHERE namespace_id = $2
          RETURNING last_capture_sequence
         )
         INSERT INTO runtime.codex_receipt_context
                (event_id, namespace_id, capture_sequence, context_version, connection_epoch,
                 frame_ordinal, transport_gap, context_document)
         SELECT $1, $2, next.last_capture_sequence, $3, $4, $5, $6, $7::text::json
           FROM next
     RETURNING capture_sequence AS "capture_sequence!""#,
        event_id,
        observation.envelope.namespace_id,
        CONTEXT_VERSION,
        context.connection_epoch(),
        context.frame_ordinal(),
        transport_gap,
        document,
    )
    .fetch_one(connection)
    .await?)
}
