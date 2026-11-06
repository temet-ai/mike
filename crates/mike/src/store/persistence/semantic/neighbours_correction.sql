WITH q AS (
    SELECT u.facet, u.vec::vector AS v FROM unnest($1::text[], $2::text[]) AS u(facet, vec)
),
qual AS (
    SELECT l.subject_id AS turn_id, 'eval.labels:' || l.id AS ref, l.tier AS qualification,
           1 AS rank, l.id AS ord
      FROM eval.labels_as_of(CASE WHEN $5 THEN 'infinity'::timestamptz ELSE $4 END) l
     WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.label = 'corrected'
       AND l.split = 'dev'
       AND (l.tier IN ('gold', 'relayed') OR ($11 = 'internal' AND l.tier = 'seed'))
    UNION ALL
    SELECT v.operator_turn_id, 'memory.turn_verdicts:' || v.id, 'live_verdict', 2, v.id
      FROM memory.turn_verdicts v
     WHERE $11 IN ('steerer', 'internal') AND v.mode = 'live' AND v.projected
       AND v.reaction = 'corrected' AND v.created_at <= $4
),
best_qual AS (
    SELECT DISTINCT ON (turn_id) turn_id, ref, qualification
      FROM qual ORDER BY turn_id, rank, ord DESC
),
pool AS (
    SELECT i.id, 'corr'::text AS item_facet, i.text, i.source_ref, i.operator_turn_id, i.session_id,
           i.project_id, i.request_id, i.effective_at, i.recorded_at, i.embedding,
           bq.ref AS qualification_ref, bq.qualification, pf.operator_turn_id AS corrected_turn_id
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
      JOIN memory.turn_facts pf
        ON (CASE WHEN $5 THEN pf.next_genuine_request_id_at_close = i.request_id
                 ELSE pf.next_genuine_turn_id = i.operator_turn_id END)
      JOIN best_qual bq ON bq.turn_id = pf.operator_turn_id
     WHERE i.kind = 'turn_prompt' AND i.embedder_key = $3
       AND i.effective_at < $4
       AND ($5 OR i.recorded_at <= $4)
       AND (CASE WHEN $5 THEN f.prompt_kind_at_close ELSE f.prompt_kind END) = 'genuine'
       AND f.session_class = 'operator'
       AND i.operator_turn_id IS DISTINCT FROM $6
       AND ($7 = 'operator' OR ($7 = 'project' AND i.project_id IS NOT DISTINCT FROM $8)
            OR ($7 = 'session' AND i.session_id = $9))
       AND pf.operator_turn_id IS DISTINCT FROM $6
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id IN (i.operator_turn_id, pf.operator_turn_id))
)
SELECT p.id AS "item_id!", p.item_facet AS "item_facet!", p.text AS "text!",
       p.source_ref AS "source_ref!", p.operator_turn_id, p.session_id, p.project_id,
       p.request_id, p.effective_at AS "effective_at!", p.recorded_at AS "recorded_at!",
       q.facet AS "query_facet!", (1 - (p.embedding <=> q.v))::float8 AS "similarity!",
       p.qualification_ref, p.qualification, p.corrected_turn_id
  FROM pool p CROSS JOIN q
 ORDER BY "similarity!" DESC, p.id
 LIMIT $10
