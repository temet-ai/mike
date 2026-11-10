use super::PersistError;
use super::recall_reads::TURN_COLUMNS;
use crate::analysis::embeddings::score_floor_for;
use sqlx::{Postgres, QueryBuilder};

const CHUNK_CANDIDATES: i64 = 200;

pub(super) async fn set_vector_scan_limits(
    tx: &mut sqlx::Transaction<'_, Postgres>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT set_config('hnsw.iterative_scan', 'relaxed_order', true) AS iterative_scan,
                set_config('hnsw.ef_search', '1000', true) AS ef_search,
                set_config('hnsw.max_scan_tuples', '100000', true) AS max_scan_tuples,
                set_config('max_parallel_workers_per_gather', '0', true) AS parallel_workers"
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn set_recall_timeouts(
    tx: &mut sqlx::Transaction<'_, Postgres>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT set_config('jit', 'off', true) AS jit,
                set_config('statement_timeout', '12s', true) AS statement_timeout"
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

pub(super) fn by_embedding_query(
    project_id: i64,
    (embedding_model, query_vec_literal): (&str, &str),
    limit: i64,
    exclude_turn: Option<i64>,
) -> QueryBuilder<Postgres> {
    let cut = score_floor_for(embedding_model);
    let mut query = QueryBuilder::new(
        "WITH nearest_chunks AS (
            SELECT c.turn_id,
                   c.chunk_embedding <=> ",
    );
    query
        .push_bind(query_vec_literal)
        .push(
            "::vector AS distance
              FROM memory.operator_turn_chunks c
              JOIN memory.operator_turns ot ON c.turn_id = ot.id
             WHERE ot.project_id = ",
        )
        .push_bind(project_id)
        .push(
            "
               AND c.embedding_model = ",
        )
        .push_bind(embedding_model)
        .push(
            "
               AND (ot.opened_by IS NULL OR ot.opened_by = 'operator')
               AND ot.id IS DISTINCT FROM ",
        )
        .push_bind(exclude_turn)
        .push(
            "
             ORDER BY c.chunk_embedding <=> $1::vector
             LIMIT ",
        )
        .push_bind(limit.max(CHUNK_CANDIDATES))
        .push(
            "
         ),
         chunk_distances AS (
            SELECT turn_id, MIN(distance) AS min_distance
              FROM nearest_chunks
             GROUP BY turn_id
         ),
         ranked AS (
            SELECT turn_id, min_distance
              FROM (
                SELECT turn_id, min_distance,
                       MIN(min_distance) OVER () AS best_distance
                  FROM chunk_distances
              ) scored
             WHERE min_distance <= LEAST(",
        )
        .push_bind(1.0 - cut.floor)
        .push("::float8, best_distance + ")
        .push_bind(cut.window)
        .push(
            "::float8)
             ORDER BY min_distance ASC
             LIMIT ",
        )
        .push_bind(limit)
        .push(
            "
         )
         SELECT ",
        )
        .push(TURN_COLUMNS)
        .push(
            ",
                (1.0 - ranked.min_distance)::real AS rank
           FROM ranked
          CROSS JOIN LATERAL (",
        )
        .push(crate::store::operator_turns::TURN_SELECT_BASE)
        .push(
            "
             WHERE t.id = ranked.turn_id
             OFFSET 0) t
          ORDER BY ranked.min_distance ASC, t.created_at DESC",
        );
    query
}
