WITH windowed AS (
    SELECT blocks, created_at
    FROM system_prompts.session_prompts
    WHERE lane = 'main'
      AND subagent_persona IS NOT DISTINCT FROM NULL
      AND created_at >= COALESCE(NULL::timestamptz, NOW() - INTERVAL '7 days')
      AND created_at <  COALESCE(NULL::timestamptz, NOW())
),
agg AS (
    SELECT COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen
    FROM windowed
),
latest AS (
    SELECT blocks AS sample_blocks
    FROM windowed
    ORDER BY created_at DESC
    LIMIT 1
),
registered AS (
    SELECT DISTINCT ON (ov.lane, ov.agent_name)
        ov.id AS composition_id, ov.label, m.code AS mode, ov.agent_definition_path
    FROM overrides.overrides ov
    JOIN overrides.modes m ON m.id = ov.mode_id
    JOIN overrides.sources src ON src.id = ov.source_id AND src.code = 'manual'
    WHERE ov.lane = 'main'
      AND ov.agent_name IS NOT DISTINCT FROM NULL
    ORDER BY ov.lane, ov.agent_name, ov.updated_at DESC
)
SELECT
    'main'::text                      AS "lane!",
    NULL::text                        AS subagent_persona,
    a.observation_count               AS "observation_count!",
    a.first_seen,
    a.last_seen,
    l.sample_blocks,
    r.composition_id,
    r.label,
    r.mode,
    r.agent_definition_path
FROM agg a
LEFT JOIN latest l ON true
LEFT JOIN registered r ON true
WHERE a.observation_count > 0 OR r.composition_id IS NOT NULL;
