-- BEFORE: naive combined form, same ARRAY_AGG-then-index anti-pattern the catalogue
-- query already had to avoid (Perf-Query-SHA history: prompt_classes). Kept only as
-- the perf-check "before" baseline that motivated the split agg/latest CTE shape in
-- catalogue-query_detail_final.sql — not the shipped query.
WITH windowed AS (
    SELECT blocks, created_at
    FROM system_prompts.session_prompts
    WHERE lane = 'main'
      AND subagent_persona IS NOT DISTINCT FROM NULL::text
      AND created_at >= COALESCE(NULL::timestamptz, NOW() - INTERVAL '7 days')
      AND created_at <  COALESCE(NULL::timestamptz, NOW())
),
agg AS (
    SELECT COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen,
           (ARRAY_AGG(blocks ORDER BY created_at DESC))[1] AS sample_blocks
    FROM windowed
),
registered AS (
    SELECT DISTINCT ON (ov.lane, ov.agent_name)
        ov.id AS composition_id, ov.label, m.code AS mode, ov.agent_definition_path
    FROM overrides.overrides ov
    JOIN overrides.modes m ON m.id = ov.mode_id
    JOIN overrides.sources src ON src.id = ov.source_id AND src.code = 'manual'
    WHERE ov.lane = 'main'
      AND ov.agent_name IS NOT DISTINCT FROM NULL::text
    ORDER BY ov.lane, ov.agent_name, ov.updated_at DESC
)
SELECT
    'main'::text AS lane,
    NULL::text AS subagent_persona,
    a.observation_count,
    a.first_seen,
    a.last_seen,
    a.sample_blocks,
    r.composition_id,
    r.label,
    r.mode,
    r.agent_definition_path
FROM agg a
LEFT JOIN registered r ON true
WHERE a.observation_count > 0 OR r.composition_id IS NOT NULL;
