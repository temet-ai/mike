WITH q AS (
    SELECT u.facet, u.vec::vector AS v FROM unnest($1::text[], $2::text[]) AS u(facet, vec)
),
pool AS (
    SELECT i.id, 'directive'::text AS item_facet, i.text, i.source_ref, i.operator_turn_id,
           i.session_id, i.project_id, i.request_id, i.effective_at, i.recorded_at, i.embedding,
           NULL::text AS qualification_ref, NULL::text AS qualification,
           NULL::bigint AS corrected_turn_id
      FROM memory.semantic_items i
     WHERE i.kind = 'directive' AND i.embedder_key = $3
       AND i.effective_at < $4
       AND ($5 OR i.recorded_at <= $4)
       AND i.operator_turn_id IS DISTINCT FROM $6
       AND ($7 = 'operator' OR ($7 = 'project' AND i.project_id IS NOT DISTINCT FROM $8)
            OR ($7 = 'session' AND i.session_id = $9))
)
SELECT p.id AS "item_id!", p.item_facet AS "item_facet!", p.text AS "text!",
       p.source_ref AS "source_ref!", p.operator_turn_id, p.session_id, p.project_id,
       p.request_id, p.effective_at AS "effective_at!", p.recorded_at AS "recorded_at!",
       q.facet AS "query_facet!", (1 - (p.embedding <=> q.v))::float8 AS "similarity!",
       p.qualification_ref, p.qualification, p.corrected_turn_id
  FROM pool p CROSS JOIN q
 ORDER BY "similarity!" DESC, p.id
 LIMIT $10
