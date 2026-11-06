-- BEFORE: blueprint's literal design (ARRAY_AGG(blocks ORDER BY created_at DESC))[1]
-- per artifacts/threads/prompts-surface-rebuild/backend-blueprint.md:321-364.
-- Kept only as the perf-check "before" baseline that motivated the DISTINCT ON
-- rewrite in catalogue_query_final.sql — not the shipped query.
WITH windowed AS (
    SELECT lane, subagent_persona, blocks, created_at,
           agent_id, instance_id
    FROM system_prompts.session_prompts
    WHERE created_at >= COALESCE(NULL::timestamptz, NOW() - INTERVAL '7 days')
      AND created_at <  COALESCE(NULL::timestamptz, NOW())
),
classes AS (
    SELECT lane, subagent_persona,
           COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen,
           (ARRAY_AGG(blocks ORDER BY created_at DESC))[1] AS sample_blocks
    FROM windowed
    GROUP BY lane, subagent_persona
),
registered AS (
    SELECT DISTINCT ON (ov.lane, ov.agent_name)
        ov.lane, ov.agent_name, ov.id AS composition_id, ov.label,
        m.code AS mode, ov.agent_definition_path
    FROM overrides.overrides ov
    JOIN overrides.modes m ON m.id = ov.mode_id
    JOIN overrides.sources src ON src.id = ov.source_id AND src.code = 'manual'
    ORDER BY ov.lane, ov.agent_name, ov.updated_at DESC
)
SELECT
    COALESCE(c.lane, r.lane) AS lane,
    COALESCE(c.subagent_persona, r.agent_name) AS subagent_persona,
    c.observation_count, c.first_seen, c.last_seen, c.sample_blocks,
    r.composition_id, r.label, r.mode, r.agent_definition_path
FROM classes c
FULL OUTER JOIN registered r
    ON r.lane = c.lane
   AND r.agent_name IS NOT DISTINCT FROM c.subagent_persona
ORDER BY COALESCE(c.last_seen, '-infinity'::timestamptz) DESC
LIMIT 50 OFFSET 0;
