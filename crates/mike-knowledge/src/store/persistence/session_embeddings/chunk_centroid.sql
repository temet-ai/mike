SELECT COUNT(*)::bigint AS "chunk_count!",
       GREATEST(MAX(otc.embedded_at), MAX(m.chunks_embedded_at)) AS "max_embedded_at?",
       CASE WHEN COUNT(*) > 0
            THEN l2_normalize(avg(otc.chunk_embedding))::text
       END AS "embedding_text?"
  FROM memory.operator_turn_chunks otc
  JOIN memory.operator_turns ot ON ot.id = otc.turn_id
  JOIN sessions.sessions s ON s.id = ot.session_id
  LEFT JOIN memory.session_chunk_marks m ON m.session_id = s.id
 WHERE s.session_uuid = $1
