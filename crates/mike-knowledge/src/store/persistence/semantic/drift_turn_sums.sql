WITH items AS (
    SELECT i.embedding, i.effective_at
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE i.kind = $3 AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
)
SELECT count(*) FILTER (WHERE effective_at >= $2::timestamptz - interval '7 days')::bigint AS "n_current!",
       (sum(embedding) FILTER (WHERE effective_at >= $2::timestamptz - interval '7 days'))::text AS current_sum,
       count(*) FILTER (WHERE effective_at < $2::timestamptz - interval '7 days')::bigint AS "n_baseline!",
       (sum(embedding) FILTER (WHERE effective_at < $2::timestamptz - interval '7 days'))::text AS baseline_sum
  FROM items
 WHERE effective_at >= $2::timestamptz - interval '35 days' AND effective_at < $2::timestamptz
