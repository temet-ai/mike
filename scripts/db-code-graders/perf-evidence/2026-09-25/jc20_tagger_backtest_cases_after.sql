SELECT p.id AS item_id,
       array_agg(DISTINCT a.pattern ORDER BY a.pattern) AS patterns,
       a.origin_session_uuid::text AS origin_session_uuid,
       a.request_id,
       p.embedding::text AS embedding
  FROM memory.semantic_items a
  JOIN memory.semantic_items p ON p.kind = 'turn_prompt'
   AND p.embedder_key = a.embedder_key
   AND p.request_id = a.request_id
 WHERE a.kind = 'anchor'
   AND a.embedder_key = 'nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:2026-09-25T10:23:37Z'
   AND a.pattern IS NOT NULL
 GROUP BY p.id, a.origin_session_uuid, a.request_id, p.embedding
 ORDER BY p.id;
