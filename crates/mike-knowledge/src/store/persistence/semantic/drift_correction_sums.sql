WITH items AS (
    SELECT i.embedding, i.effective_at
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
      JOIN memory.turn_facts pf ON pf.next_genuine_turn_id = i.operator_turn_id
     WHERE i.kind = 'turn_prompt' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND (EXISTS (
               SELECT 1 FROM eval.labels_as_of($2) l
                WHERE l.subject_kind = 'turn' AND l.subject_id = pf.operator_turn_id
                  AND l.axis = 'reaction' AND l.label = 'corrected' AND l.split = 'dev'
                  AND l.tier IN ('gold', 'relayed', 'seed'))
            OR EXISTS (
               SELECT 1 FROM memory.turn_verdicts v
                WHERE v.operator_turn_id = pf.operator_turn_id AND v.mode = 'live'
                  AND v.projected AND v.reaction = 'corrected' AND v.created_at <= $2))
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id IN (i.operator_turn_id, pf.operator_turn_id))
)
SELECT count(*) FILTER (WHERE effective_at >= $2 - interval '7 days')::bigint AS "n_current!",
       (sum(embedding) FILTER (WHERE effective_at >= $2 - interval '7 days'))::text AS current_sum,
       count(*) FILTER (WHERE effective_at < $2 - interval '7 days')::bigint AS "n_baseline!",
       (sum(embedding) FILTER (WHERE effective_at < $2 - interval '7 days'))::text AS baseline_sum
  FROM items
 WHERE effective_at >= $2 - interval '35 days' AND effective_at < $2
