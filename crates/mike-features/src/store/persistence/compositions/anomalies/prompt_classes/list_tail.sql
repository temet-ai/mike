),
classes AS (
    SELECT lane, subagent_persona,
           COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen
    FROM windowed
    GROUP BY lane, subagent_persona
),
latest_blocks AS (
    SELECT DISTINCT ON (lane, subagent_persona)
           lane, subagent_persona, blocks AS sample_blocks
    FROM windowed
    ORDER BY lane, subagent_persona, created_at DESC
),
model_breakdown AS (
    SELECT lane, subagent_persona,
           jsonb_agg(
               jsonb_build_object('model', model, 'count', model_count)
               ORDER BY model_count DESC, model
           ) AS model_breakdown
    FROM (
        SELECT lane, subagent_persona, model, COUNT(*)::bigint AS model_count
        FROM windowed
        GROUP BY lane, subagent_persona, model
    ) model_counts
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
    COALESCE(c.lane, r.lane)                          AS lane,
    COALESCE(c.subagent_persona, r.agent_name)        AS subagent_persona,
    COALESCE(c.observation_count, 0)                  AS observation_count,
    c.first_seen,
    c.last_seen,
    COALESCE(jsonb_array_length(lb.sample_blocks), 0) AS block_count,
    COALESCE(mb.model_breakdown, '[]'::jsonb)         AS model_breakdown,
    r.composition_id,
    r.label,
    r.mode,
    r.agent_definition_path
FROM classes c
LEFT JOIN latest_blocks lb
    ON lb.lane = c.lane
   AND lb.subagent_persona IS NOT DISTINCT FROM c.subagent_persona
LEFT JOIN model_breakdown mb
    ON mb.lane = c.lane
   AND mb.subagent_persona IS NOT DISTINCT FROM c.subagent_persona
FULL OUTER JOIN registered r
    ON r.lane = c.lane
   AND r.agent_name IS NOT DISTINCT FROM c.subagent_persona
ORDER BY COALESCE(c.last_seen, '-infinity'::timestamptz) DESC
