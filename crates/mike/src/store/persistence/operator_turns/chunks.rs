use std::collections::HashMap;

use sqlx::postgres::Postgres;

use super::PendingChunk;
use super::embedding::vector_to_text_literal;
use crate::store::persistence::{PersistError, PersistHandle};

const CHUNK_INSERT_BATCH_ROWS: usize = 64;

struct ChunkRow {
    chunk_idx: i32,
    chunk_role_id: i64,
    chunk_text: String,
    vec_literal: String,
}

impl PersistHandle {
    pub async fn replace_turn_chunks(
        &self,
        turn_id: i64,
        plan: &[PendingChunk],
        vectors: &[Vec<f32>],
        model: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("replace_turn_chunks", async move {
            let role_ids = resolve_chunk_role_ids(&self.pool, plan).await?;
            let rows = build_chunk_rows(plan, vectors, &role_ids)?;
            let mut batches = rows.chunks(CHUNK_INSERT_BATCH_ROWS);

            let mut tx = self.pool.begin().await?;
            sqlx::query!(
                "DELETE FROM memory.operator_turn_chunks WHERE turn_id = $1",
                turn_id,
            )
            .execute(&mut *tx)
            .await?;
            if let Some(first_batch) = batches.next() {
                insert_chunk_batch(&mut tx, turn_id, first_batch, model).await?;
            }
            tx.commit().await?;

            for batch in batches {
                let mut tx = self.pool.begin().await?;
                insert_chunk_batch(&mut tx, turn_id, batch, model).await?;
                tx.commit().await?;
            }

            Ok(())
        })
        .await
    }
}

async fn resolve_chunk_role_ids(
    pool: &sqlx::PgPool,
    plan: &[PendingChunk],
) -> Result<HashMap<&'static str, i64>, PersistError> {
    let mut codes: Vec<&'static str> = plan.iter().map(|chunk| chunk.chunk_role).collect();
    codes.sort_unstable();
    codes.dedup();
    if codes.is_empty() {
        return Ok(HashMap::new());
    }

    let wanted: Vec<String> = codes.iter().map(|code| (*code).to_owned()).collect();
    let found: HashMap<String, i64> = sqlx::query!(
        "SELECT code, id FROM memory.chunk_roles WHERE code = ANY($1)",
        &wanted,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.code, row.id))
    .collect();

    let mut role_ids = HashMap::with_capacity(codes.len());
    for code in codes {
        let Some(role_id) = found.get(code) else {
            return Err(PersistError::DataInvariant(format!(
                "unknown chunk_role: {code}"
            )));
        };
        role_ids.insert(code, *role_id);
    }
    Ok(role_ids)
}

fn build_chunk_rows(
    plan: &[PendingChunk],
    vectors: &[Vec<f32>],
    role_ids: &HashMap<&'static str, i64>,
) -> Result<Vec<ChunkRow>, PersistError> {
    plan.iter()
        .zip(vectors.iter())
        .map(|(chunk, vector)| {
            let chunk_role_id = *role_ids.get(chunk.chunk_role).ok_or_else(|| {
                PersistError::DataInvariant(format!("unknown chunk_role: {}", chunk.chunk_role))
            })?;
            Ok(ChunkRow {
                chunk_idx: chunk.chunk_idx,
                chunk_role_id,
                chunk_text: chunk.chunk_text.clone(),
                vec_literal: vector_to_text_literal(vector),
            })
        })
        .collect()
}

async fn insert_chunk_batch(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    turn_id: i64,
    batch: &[ChunkRow],
    model: &str,
) -> Result<(), PersistError> {
    if batch.is_empty() {
        return Ok(());
    }
    let chunk_idx: Vec<i32> = batch.iter().map(|row| row.chunk_idx).collect();
    let chunk_role_id: Vec<i64> = batch.iter().map(|row| row.chunk_role_id).collect();
    let chunk_text: Vec<String> = batch.iter().map(|row| row.chunk_text.clone()).collect();
    let chunk_embedding: Vec<String> = batch.iter().map(|row| row.vec_literal.clone()).collect();
    sqlx::query!(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT $1, v.chunk_idx, v.chunk_role_id, v.chunk_text, v.chunk_embedding::vector, $2
           FROM unnest($3::int4[], $4::int8[], $5::text[], $6::text[])
                AS v(chunk_idx, chunk_role_id, chunk_text, chunk_embedding)",
        turn_id,
        model,
        &chunk_idx,
        &chunk_role_id,
        &chunk_text,
        &chunk_embedding,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
