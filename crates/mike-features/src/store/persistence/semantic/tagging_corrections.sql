WITH qual AS (
    SELECT l.subject_id AS turn_id, 'eval.labels:' || l.id AS ref, l.tier AS qualification,
           1 AS rank, l.id AS ord
      FROM eval.labels_as_of('infinity'::timestamptz) l
     WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.label = 'corrected'
       AND l.split = 'dev' AND l.tier IN ('gold', 'relayed', 'seed')
    UNION ALL
    SELECT v.operator_turn_id, 'memory.turn_verdicts:' || v.id, 'live_verdict', 2, v.id
      FROM memory.turn_verdicts v
     WHERE v.mode = 'live' AND v.projected AND v.reaction = 'corrected'
),
best_qual AS (
    SELECT DISTINCT ON (turn_id) turn_id, ref, qualification FROM qual ORDER BY turn_id, rank, ord DESC
)
SELECT i.id AS item_id, pf.operator_turn_id AS corrected_turn_id, s.session_uuid::text AS "session_uuid!",
       i.request_id, bq.qualification AS "qualification!", bq.ref AS "qualification_ref!", i.embedding::text AS "embedding!"
  FROM memory.semantic_items i
  JOIN sessions.sessions s ON s.id = i.session_id
  JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
  JOIN memory.turn_facts pf ON pf.next_genuine_turn_id = i.operator_turn_id
  JOIN best_qual bq ON bq.turn_id = pf.operator_turn_id
 WHERE i.kind = 'turn_prompt' AND i.embedder_key = $1
   AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
   AND NOT EXISTS (
       SELECT 1 FROM eval.turn_splits sp
        WHERE sp.split = 'holdout' AND sp.operator_turn_id IN (i.operator_turn_id, pf.operator_turn_id))
   AND NOT EXISTS (
       SELECT 1 FROM memory.pattern_assignments a
        WHERE a.correction_item_id = i.id AND a.tagger_version = $2 AND a.embedder_key = $1)
 ORDER BY i.id
 LIMIT $3
