WITH chunk_agg AS (
       SELECT ot.session_id AS session_db_id,
              MAX(otc.embedded_at) AS max_chunk_embedded_at
         FROM memory.operator_turn_chunks otc
         JOIN memory.operator_turns ot ON ot.id = otc.turn_id
        GROUP BY ot.session_id
   )
   SELECT s.session_uuid          AS "session_id!",
          s.overview               AS "overview?",
          s.overview_generated_at   AS "overview_generated_at?",
          ca.max_chunk_embedded_at  AS "max_chunk_embedded_at?"
     FROM sessions.sessions s
     JOIN chunk_agg ca ON ca.session_db_id = s.id
     LEFT JOIN memory.session_embeddings se ON se.session_id = s.session_uuid
    WHERE se.session_id IS NULL
       OR se.source_updated_at < GREATEST(
            COALESCE(ca.max_chunk_embedded_at, '-infinity'::timestamptz),
            COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
          )
    ORDER BY GREATEST(
            COALESCE(ca.max_chunk_embedded_at, '-infinity'::timestamptz),
            COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
          ) ASC
    LIMIT 64;
