WITH windowed AS MATERIALIZED (
    SELECT sp.request_id, sp.session_id, sp.created_at, r.model,
           sp.cc_release AS release, sp.cc_build AS build
    FROM system_prompts.session_prompts sp
    LEFT JOIN requests.requests r ON r.id = sp.request_id
    WHERE sp.lane = 'main'
      AND COALESCE(sp.subagent_persona, '') = COALESCE(NULL::text, '')
),
releases AS (
    SELECT release,
           COUNT(DISTINCT build)::bigint AS builds,
           COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen
    FROM windowed
    GROUP BY release
),
per_model AS (
    SELECT release, model,
           COUNT(*)::bigint AS observation_count,
           MIN(created_at) AS first_seen,
           MAX(created_at) AS last_seen
    FROM windowed
    GROUP BY release, model
),
latest_per_model AS (
    SELECT DISTINCT ON (release, model)
           release, model, request_id, session_id
    FROM windowed
    ORDER BY release, model, created_at DESC
)
SELECT
    r.release,
    r.builds AS "builds!",
    r.observation_count AS "release_observation_count!",
    r.first_seen AS "release_first_seen!",
    r.last_seen AS "release_last_seen!",
    pm.model,
    pm.observation_count AS "model_observation_count!",
    pm.first_seen AS "model_first_seen!",
    pm.last_seen AS "model_last_seen!",
    lpm.request_id AS "latest_request_id!",
    lpm.session_id AS "latest_session_id!"
FROM releases r
JOIN per_model pm ON pm.release IS NOT DISTINCT FROM r.release
JOIN latest_per_model lpm
    ON lpm.release IS NOT DISTINCT FROM pm.release
   AND lpm.model IS NOT DISTINCT FROM pm.model
ORDER BY r.last_seen DESC, pm.observation_count DESC, pm.model;
