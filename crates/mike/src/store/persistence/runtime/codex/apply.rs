use sqlx::PgConnection;
use uuid::Uuid;

use crate::runtime::codex::{ProjectionBatch, decode_observation};
use crate::store::persistence::{PersistError, PersistHandle};

use super::effects::{insert_effects, validate_batch};
use super::generation::{check_versions, lock_generation};
use super::reads::ReceiptRow;
use super::state::reduce_states;
use super::types::status_code;
use super::{
    CodexGeneration, CodexProjectionApplication, CodexProjectionApplyOutcome, StoredRuntimeReceipt,
    invariant,
};

impl PersistHandle {
    pub async fn apply_next_codex_projection(
        &self,
        generation_id: Uuid,
    ) -> Result<CodexProjectionApplyOutcome, PersistError> {
        let mut transaction = self.pool.begin().await?;
        let generation = lock_generation(&mut transaction, generation_id).await?;
        check_versions(&generation)?;
        if !matches!(generation.state.as_str(), "building" | "active") {
            return Err(invariant("generation is not writable"));
        }
        let Some(receipt) = next_receipt(&mut transaction, &generation).await? else {
            transaction.commit().await?;
            return Ok(CodexProjectionApplyOutcome::CaughtUp(generation));
        };
        let sequence = receipt
            .capture_sequence
            .ok_or_else(|| invariant("receipt has no trusted capture sequence"))?;
        if generation.last_applied_capture_sequence.checked_add(1) != Some(sequence) {
            return Err(invariant(
                "generation application sequence is discontinuous",
            ));
        }
        let context = receipt
            .context
            .as_ref()
            .ok_or_else(|| invariant("legacy receipt has no trusted context"))?;
        context.validate()?;
        let scope = context.scope();
        let batch = decode_observation(&receipt.observation, context.borrowed(scope.as_ref()));
        validate_batch(&batch, &receipt)?;
        insert_effects(&mut transaction, &generation, &receipt, &batch).await?;
        reduce_states(&mut transaction, &generation, sequence, &batch).await?;
        let application =
            record_application(&mut transaction, &generation, &receipt, batch).await?;
        transaction.commit().await?;
        Ok(CodexProjectionApplyOutcome::Applied(application))
    }
}

async fn next_receipt(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
) -> Result<Option<StoredRuntimeReceipt>, PersistError> {
    let row = sqlx::query_file_as!(
        ReceiptRow,
        "src/store/persistence/runtime/codex/next_receipt.sql",
        generation.namespace_id,
        generation.generation_id,
        generation.last_applied_capture_sequence,
    )
    .fetch_optional(connection)
    .await?;
    row.map(TryInto::try_into).transpose()
}

async fn record_application(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
    receipt: &StoredRuntimeReceipt,
    batch: ProjectionBatch,
) -> Result<CodexProjectionApplication, PersistError> {
    let sequence = receipt
        .capture_sequence
        .ok_or_else(|| invariant("application has no capture sequence"))?;
    let effect_count =
        i32::try_from(batch.effects.len()).map_err(|_| invariant("too many decoded effects"))?;
    let issues = serde_json::to_string(&batch.issues)?;
    sqlx::query!(
        "WITH applied AS (
             INSERT INTO runtime.codex_projection_applications
                    (generation_id, namespace_id, event_id, capture_sequence, status, issues, effect_count)
             VALUES ($1, $2, $3, $4, $5, $6::text::json, $7)
             RETURNING generation_id, capture_sequence
         )
         UPDATE runtime.codex_projection_generations g
            SET last_applied_capture_sequence = applied.capture_sequence
           FROM applied
          WHERE g.generation_id = applied.generation_id",
        generation.generation_id,
        generation.namespace_id,
        receipt.event_id,
        sequence,
        status_code(batch.status),
        issues,
        effect_count,
    )
    .execute(connection)
    .await?;
    Ok(CodexProjectionApplication {
        generation_id: generation.generation_id,
        event_id: receipt.event_id,
        capture_sequence: sequence,
        status: batch.status,
        issues: batch.issues,
        effect_count,
    })
}
