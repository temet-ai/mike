WITH c AS (
    SELECT u.item_id, u.session_uuid, u.request_id, u.vec::vector AS v
      FROM unnest($1::bigint[], $2::text[], $3::bigint[], $4::text[]) AS u(item_id, session_uuid, request_id, vec)
)
SELECT c.item_id AS "correction_item_id!", a.id AS "anchor_item_id!", a.pattern AS pattern,
       a.origin_session_uuid::text AS origin_session_uuid, a.request_id,
       a.similarity AS "similarity!"
  FROM c
  CROSS JOIN LATERAL (
      SELECT x.id, x.pattern, x.origin_session_uuid, x.request_id,
             (1 - (x.embedding <=> c.v))::float8 AS similarity
        FROM memory.semantic_items x
       WHERE x.kind = 'anchor' AND x.embedder_key = $5
         AND x.origin_session_uuid::text IS DISTINCT FROM c.session_uuid
         AND (c.request_id IS NULL OR x.request_id IS DISTINCT FROM c.request_id)
       ORDER BY x.embedding <=> c.v, x.id
       LIMIT 10
  ) a
 ORDER BY c.item_id, a.similarity DESC
