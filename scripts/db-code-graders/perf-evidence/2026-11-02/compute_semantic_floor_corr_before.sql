WITH fa AS (
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
      JOIN memory.turn_facts pf ON pf.next_genuine_turn_id = i.operator_turn_id
     WHERE i.kind = 'turn_prompt' AND i.embedder_key = 'nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:2026-09-25T10:23:37Z'
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND (EXISTS (
               SELECT 1 FROM eval.labels_as_of('infinity'::timestamptz) l
                WHERE l.subject_kind = 'turn' AND l.subject_id = pf.operator_turn_id
                  AND l.axis = 'reaction' AND l.label = 'corrected' AND l.split = 'dev'
                  AND l.tier IN ('gold', 'relayed', 'seed'))
            OR EXISTS (
               SELECT 1 FROM memory.turn_verdicts v
                WHERE v.operator_turn_id = pf.operator_turn_id AND v.mode = 'live'
                  AND v.projected AND v.reaction = 'corrected'))
       AND NOT EXISTS (
           SELECT 1 FROM eval.turn_splits s
            WHERE s.split = 'holdout' AND s.operator_turn_id IN (i.operator_turn_id, pf.operator_turn_id))
),
fb AS (
    SELECT i.id, i.session_id, i.embedding
      FROM memory.semantic_items i
      JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
     WHERE i.kind = 'turn_prompt' AND i.embedder_key = 'nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:2026-09-25T10:23:37Z'
       AND f.prompt_kind = 'genuine' AND f.session_class = 'operator'
       AND i.effective_at > now() - interval '60 days'
),
sa AS (
    SELECT id, session_id, embedding FROM fa ORDER BY md5(id::text || 'floors-v1'), id LIMIT 300
),
sb AS (
    SELECT id, session_id, embedding FROM fb ORDER BY md5(id::text || 'floors-v1-b'), id LIMIT 300
),
pairs AS (
    SELECT (1 - (sa.embedding <=> sb.embedding))::float8 AS s
      FROM sa JOIN sb ON sa.session_id IS DISTINCT FROM sb.session_id AND sa.id <> sb.id
     LIMIT 40000
)
SELECT count(*)::bigint AS n_pairs,
       percentile_cont(0.5) WITHIN GROUP (ORDER BY s) AS p50,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY s) AS p95,
       percentile_cont(0.99) WITHIN GROUP (ORDER BY s) AS p99,
       percentile_cont(0.999) WITHIN GROUP (ORDER BY s) AS p999,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY s) AS floor
  FROM pairs
 WHERE 'nomic-embed-text-v1.5'::text IS NOT NULL;
