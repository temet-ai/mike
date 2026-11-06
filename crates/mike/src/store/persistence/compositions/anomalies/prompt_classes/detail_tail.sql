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
model_breakdown AS (
    SELECT jsonb_agg(
               jsonb_build_object('model', model, 'count', model_count)
               ORDER BY model_count DESC, model
           ) AS model_breakdown
    FROM (
        SELECT model, COUNT(*)::bigint AS model_count
        FROM windowed
        GROUP BY model
    ) model_counts
),
registered AS (
    SELECT DISTINCT ON (ov.lane, ov.agent_name)
        ov.id AS composition_id, ov.label, m.code AS mode, ov.agent_definition_path
    FROM overrides.overrides ov
    JOIN overrides.modes m ON m.id = ov.mode_id
    JOIN overrides.sources src ON src.id = ov.source_id AND src.code = 'manual'
    WHERE ov.lane = $1
      AND ov.agent_name IS NOT DISTINCT FROM $2
    ORDER BY ov.lane, ov.agent_name, ov.updated_at DESC
)
SELECT
    $1::text                          AS lane,
    $2::text                          AS subagent_persona,
    a.observation_count               AS observation_count,
    a.first_seen,
    a.last_seen,
    l.sample_blocks,
    COALESCE(mb.model_breakdown, '[]'::jsonb) AS model_breakdown,
    r.composition_id,
    r.label,
    r.mode,
    r.agent_definition_path
FROM agg a
LEFT JOIN latest l ON true
LEFT JOIN model_breakdown mb ON true
LEFT JOIN registered r ON true
WHERE a.observation_count > 0 OR r.composition_id IS NOT NULL
