INSERT INTO memory.project_embeddings
    (project_id, embedding, embedding_model, session_count, embedded_at)
SELECT s.project_id,
       l2_normalize(avg(se.embedding)),
       max(se.embedding_model),
       count(*)::int,
       now()
  FROM memory.session_embeddings se
  JOIN sessions.sessions s ON s.session_uuid = se.session_id
  LEFT JOIN memory.project_embeddings pe ON pe.project_id = s.project_id
 GROUP BY s.project_id
HAVING max(se.embedded_at) > COALESCE(max(pe.embedded_at), '-infinity'::timestamptz)
ON CONFLICT (project_id) DO UPDATE
   SET embedding        = EXCLUDED.embedding,
       embedding_model  = EXCLUDED.embedding_model,
       session_count    = EXCLUDED.session_count,
       embedded_at      = EXCLUDED.embedded_at
RETURNING project_id AS "project_id!"
