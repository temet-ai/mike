SELECT s.session_uuid          AS "session_id!",
       s.overview               AS "overview?",
       s.overview_generated_at   AS "overview_generated_at?",
       m.chunks_embedded_at      AS "max_chunk_embedded_at?"
  FROM memory.session_chunk_marks m
  JOIN sessions.sessions s ON s.id = m.session_id
  LEFT JOIN memory.session_embeddings se ON se.session_id = s.session_uuid
 WHERE (se.session_id IS NULL
        OR se.source_updated_at < GREATEST(
             m.chunks_embedded_at,
             COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
           ))
   AND EXISTS (
       SELECT 1
         FROM memory.operator_turns ot
         JOIN memory.operator_turn_chunks otc ON otc.turn_id = ot.id
        WHERE ot.session_id = m.session_id
   )
 ORDER BY GREATEST(
         m.chunks_embedded_at,
         COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
       ) ASC
 LIMIT 64;
