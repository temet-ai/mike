WITH fa AS (
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $8::text = 'prompt'
       AND i.kind = 'turn_prompt' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND i.effective_at > now() - interval '60 days'
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $8::text = 'ending'
       AND i.kind = 'turn_ending' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND i.effective_at > now() - interval '60 days'
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.turn_facts pf
      JOIN memory.semantic_items i ON i.operator_turn_id = pf.next_genuine_turn_id
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $8::text = 'corr'
       AND pf.operator_turn_id IN (
               SELECT l.subject_id FROM eval.labels_as_of('infinity'::timestamptz) l
                WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.label = 'corrected'
                  AND l.split = 'dev' AND l.tier IN ('gold', 'relayed', 'seed')
               UNION
               SELECT v.operator_turn_id FROM memory.turn_verdicts v
                WHERE v.mode = 'live' AND v.projected AND v.reaction = 'corrected')
       AND i.kind = 'turn_prompt' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id = i.operator_turn_id)
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id = pf.operator_turn_id)
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
     WHERE $8::text = 'directive'
       AND i.kind = 'directive' AND i.embedder_key = $1
    UNION ALL
    SELECT l.id, l.session_id, l.embedding
      FROM memory.lessons l
     WHERE $8::text = 'lesson'
       AND l.embedding IS NOT NULL AND l.embedding_model = $5 AND l.archived_at IS NULL
),
fb AS (
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $9::text = 'prompt'
       AND i.kind = 'turn_prompt' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND i.effective_at > now() - interval '60 days'
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $9::text = 'ending'
       AND i.kind = 'turn_ending' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND i.effective_at > now() - interval '60 days'
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.turn_facts pf
      JOIN memory.semantic_items i ON i.operator_turn_id = pf.next_genuine_turn_id
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE $9::text = 'corr'
       AND pf.operator_turn_id IN (
               SELECT l.subject_id FROM eval.labels_as_of('infinity'::timestamptz) l
                WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.label = 'corrected'
                  AND l.split = 'dev' AND l.tier IN ('gold', 'relayed', 'seed')
               UNION
               SELECT v.operator_turn_id FROM memory.turn_verdicts v
                WHERE v.mode = 'live' AND v.projected AND v.reaction = 'corrected')
       AND i.kind = 'turn_prompt' AND i.embedder_key = $1
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id = i.operator_turn_id)
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id = pf.operator_turn_id)
    UNION ALL
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
     WHERE $9::text = 'directive'
       AND i.kind = 'directive' AND i.embedder_key = $1
    UNION ALL
    SELECT l.id, l.session_id, l.embedding
      FROM memory.lessons l
     WHERE $9::text = 'lesson'
       AND l.embedding IS NOT NULL AND l.embedding_model = $5 AND l.archived_at IS NULL
),
sa AS MATERIALIZED (
    SELECT id, session_id, embedding::real[]::vector AS embedding
      FROM (SELECT id, session_id, embedding FROM fa ORDER BY md5(id::text || $2), id LIMIT $3) picked
),
sb AS MATERIALIZED (
    SELECT id, session_id, embedding::real[]::vector AS embedding
      FROM (SELECT id, session_id, embedding FROM fb ORDER BY md5(id::text || $4), id LIMIT $3) picked
),
pairs AS (
    SELECT (1 - (sa.embedding <=> sb.embedding))::float8 AS s
      FROM sa JOIN sb ON sa.session_id IS DISTINCT FROM sb.session_id AND sa.id <> sb.id
     LIMIT $6
)
SELECT count(*)::bigint AS "n_pairs!",
       percentile_cont(0.5) WITHIN GROUP (ORDER BY s) AS p50,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY s) AS p95,
       percentile_cont(0.99) WITHIN GROUP (ORDER BY s) AS p99,
       percentile_cont(0.999) WITHIN GROUP (ORDER BY s) AS p999,
       percentile_cont($7) WITHIN GROUP (ORDER BY s) AS floor
  FROM pairs
