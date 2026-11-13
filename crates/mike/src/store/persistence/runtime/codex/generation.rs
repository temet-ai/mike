use sqlx::PgConnection;
use uuid::Uuid;

use crate::store::persistence::PersistError;

use super::super::RuntimeWriteOutcome;
use super::codec::STATE_CODEC_VERSION;
use super::intake::lock_intake;
use super::{CodexGeneration, PROJECTOR_VERSION, invariant};

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn begin_codex_projection_generation(
        self,
        namespace_id: Uuid,
        generation_id: Uuid,
    ) -> Result<CodexGeneration, PersistError> {
        if generation_id.is_nil() {
            return Err(invariant("generation ID must not be nil"));
        }
        let mut transaction = self.pool.begin().await?;
        lock_intake(&mut transaction, namespace_id).await?;
        sqlx::query!(
            "INSERT INTO runtime.codex_projection_generations
             (generation_id, namespace_id, projector_version, state_codec_version, state)
             VALUES ($1, $2, $3, $4, 'building') ON CONFLICT (generation_id) DO NOTHING",
            generation_id,
            namespace_id,
            PROJECTOR_VERSION,
            STATE_CODEC_VERSION,
        )
        .execute(&mut *transaction)
        .await?;
        let generation = lock_generation(&mut transaction, generation_id).await?;
        if generation.namespace_id != namespace_id {
            return Err(invariant("generation belongs to a different namespace"));
        }
        check_versions(&generation)?;
        sqlx::query!(
            "INSERT INTO runtime.codex_projection_heads (namespace_id) VALUES ($1)
             ON CONFLICT (namespace_id) DO NOTHING",
            namespace_id,
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(generation)
    }

    pub async fn codex_projection_generation(
        self,
        generation_id: Uuid,
    ) -> Result<Option<CodexGeneration>, PersistError> {
        Ok(sqlx::query_as!(
            CodexGeneration,
            "SELECT generation_id, namespace_id, projector_version, state_codec_version, state,
                    last_applied_capture_sequence
               FROM runtime.codex_projection_generations WHERE generation_id = $1",
            generation_id,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn active_codex_projection_generation(
        self,
        namespace_id: Uuid,
    ) -> Result<Option<CodexGeneration>, PersistError> {
        Ok(sqlx::query_as!(
            CodexGeneration,
            "SELECT generation.generation_id, generation.namespace_id, generation.projector_version,
                    generation.state_codec_version, generation.state, generation.last_applied_capture_sequence
               FROM runtime.codex_projection_heads head
               JOIN runtime.codex_projection_generations generation
                 ON generation.generation_id = head.active_generation_id
                AND generation.namespace_id = head.namespace_id
              WHERE head.namespace_id = $1",
            namespace_id,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn activate_codex_projection_generation(
        self,
        generation_id: Uuid,
    ) -> Result<RuntimeWriteOutcome, PersistError> {
        let initial = self
            .codex_projection_generation(generation_id)
            .await?
            .ok_or_else(|| invariant("generation does not exist"))?;
        let mut transaction = self.pool.begin().await?;
        let boundary = lock_intake(&mut transaction, initial.namespace_id).await?;
        let active = sqlx::query_scalar!(
            r#"SELECT active_generation_id AS "active_generation_id?" FROM runtime.codex_projection_heads
             WHERE namespace_id = $1 FOR UPDATE"#,
            initial.namespace_id,
        )
        .fetch_one(&mut *transaction)
        .await?;
        let generation = lock_generation(&mut transaction, generation_id).await?;
        check_versions(&generation)?;
        if active == Some(generation_id) {
            transaction.commit().await?;
            return Ok(RuntimeWriteOutcome::Unchanged);
        }
        if generation.state != "building" || generation.last_applied_capture_sequence != boundary {
            return Err(invariant(
                "only a fully caught-up building generation can be activated",
            ));
        }
        check_complete(&mut transaction, &generation).await?;
        promote(&mut transaction, &generation, active).await?;
        transaction.commit().await?;
        Ok(RuntimeWriteOutcome::Inserted)
    }
}

pub(super) async fn lock_generation(
    connection: &mut PgConnection,
    generation_id: Uuid,
) -> Result<CodexGeneration, PersistError> {
    sqlx::query_as!(
        CodexGeneration,
        "SELECT generation_id, namespace_id, projector_version, state_codec_version, state,
                last_applied_capture_sequence
           FROM runtime.codex_projection_generations WHERE generation_id = $1 FOR UPDATE",
        generation_id,
    )
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invariant("generation does not exist"))
}

pub(super) fn check_versions(generation: &CodexGeneration) -> Result<(), PersistError> {
    if generation.projector_version != PROJECTOR_VERSION
        || generation.state_codec_version != STATE_CODEC_VERSION
    {
        return Err(invariant(
            "unsupported projector or item state codec version",
        ));
    }
    Ok(())
}

async fn check_complete(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
) -> Result<(), PersistError> {
    let incomplete = sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM runtime.codex_receipt_context context
              WHERE context.namespace_id = $1
                AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_applications application
                                 WHERE application.generation_id = $2
                                   AND application.event_id = context.event_id)
         ) AS "incomplete!""#,
        generation.namespace_id,
        generation.generation_id,
    )
    .fetch_one(connection)
    .await?;
    if incomplete {
        return Err(invariant("generation has unapplied receipt context"));
    }
    Ok(())
}

async fn promote(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
    previous: Option<Uuid>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "WITH retired AS (
             UPDATE runtime.codex_projection_generations SET state = 'retired' WHERE generation_id = $1
         ), activated AS (
             UPDATE runtime.codex_projection_generations SET state = 'active', activated_at = now()
              WHERE generation_id = $2
         )
         UPDATE runtime.codex_projection_heads SET active_generation_id = $2 WHERE namespace_id = $3",
        previous,
        generation.generation_id,
        generation.namespace_id,
    )
    .execute(connection)
    .await?;
    Ok(())
}
