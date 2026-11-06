WITH q AS (
    SELECT u.facet, u.vec::vector AS v FROM unnest($1::text[], $2::text[]) AS u(facet, vec)
),
pool AS (
    SELECT l.id, 'lesson'::text AS item_facet, l.text, 'memory.lessons:' || l.id AS source_ref,
           l.source_turn_id AS operator_turn_id, l.session_id, l.project_id, l.request_id,
           l.created_at AS effective_at, l.created_at AS recorded_at, l.embedding,
           NULL::text AS qualification_ref, NULL::text AS qualification,
           NULL::bigint AS corrected_turn_id
      FROM memory.lessons l
     WHERE l.embedding IS NOT NULL AND l.embedding_model = $3 AND l.archived_at IS NULL
       AND l.created_at < $4
       AND ($5 OR l.created_at <= $4)
       AND (l.expires_at IS NULL OR l.expires_at > $4)
       AND l.source_turn_id IS DISTINCT FROM $6
       AND ($7 = 'operator' OR ($7 = 'project' AND l.project_id IS NOT DISTINCT FROM $8)
            OR ($7 = 'session' AND l.session_id = $9))
)
SELECT p.id AS "item_id!", p.item_facet AS "item_facet!", p.text AS "text!",
       p.source_ref AS "source_ref!", p.operator_turn_id, p.session_id, p.project_id,
       p.request_id, p.effective_at AS "effective_at!", p.recorded_at AS "recorded_at!",
       q.facet AS "query_facet!", (1 - (p.embedding <=> q.v))::float8 AS "similarity!",
       p.qualification_ref, p.qualification, p.corrected_turn_id
  FROM pool p CROSS JOIN q
 ORDER BY "similarity!" DESC, p.id
 LIMIT $10
