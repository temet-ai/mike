WITH base AS (
    SELECT
        r.id AS request_id,
        sess.session_uuid::text AS session_id,
        r.created_at,
        rsp.id AS system_prompt_id,
        NULL::text AS prompt_sha256,
        'structural-mismatch'::text AS classification,
        rsp.unknown_blocks
    FROM requests.requests r
    JOIN sessions.sessions sess
        ON sess.id = r.session_id
    JOIN requests.system_prompts rsp
        ON rsp.request_id = r.id
    WHERE rsp.unknown_blocks IS NOT NULL
      AND r.created_at >= '2026-07-21 22:03:27.492043+00'::timestamptz
      AND r.created_at < '2026-07-28 22:03:27.492043+00'::timestamptz
      AND NOT EXISTS (
          SELECT 1 FROM requests.system_prompt_firing_rules j
          JOIN rules.rules fr ON fr.id = j.filter_rule_id
          JOIN overrides.overrides pc ON pc.filter_rule_id = fr.id
          JOIN overrides.sources src ON src.id = pc.source_id AND src.code = 'filter'
          WHERE j.request_system_prompt_id = rsp.id
      )
    UNION ALL
    SELECT
        r.id AS request_id,
        sess.session_uuid::text AS session_id,
        r.created_at,
        rsp.id AS system_prompt_id,
        rsp.prompt_sha256 AS prompt_sha256,
        'unknown-class'::text AS classification,
        NULL::jsonb AS unknown_blocks
    FROM requests.requests r
    JOIN sessions.sessions sess
        ON sess.id = r.session_id
    JOIN requests.system_prompts rsp
        ON rsp.request_id = r.id
    WHERE rsp.prompt_sha256 IS NOT NULL
      AND r.created_at >= '2026-07-21 22:03:27.492043+00'::timestamptz
      AND r.created_at < '2026-07-28 22:03:27.492043+00'::timestamptz
      AND NOT EXISTS (
          SELECT 1 FROM overrides.overrides pc
          JOIN overrides.sources src ON src.id = pc.source_id AND src.code = 'manual'
          WHERE pc.prompt_sha256 = rsp.prompt_sha256
      )
),
firing AS (
    SELECT
        b.classification,
        b.prompt_sha256,
        b.unknown_blocks,
        ARRAY_AGG(DISTINCT fr.uuid::text) AS firing_rule_ids
    FROM base b
    JOIN requests.system_prompt_firing_rules j
        ON j.request_system_prompt_id = b.system_prompt_id
    JOIN rules.rules fr ON fr.id = j.filter_rule_id
    GROUP BY b.classification, b.prompt_sha256, b.unknown_blocks
),
grouped AS (
    SELECT
        b.classification,
        b.prompt_sha256,
        b.unknown_blocks,
        COUNT(*)::bigint AS occurrence_count,
        MIN(b.created_at) AS first_seen,
        MAX(b.created_at) AS last_seen,
        jsonb_agg(
            jsonb_build_object('request_id', b.request_id, 'session_id', b.session_id)
            ORDER BY b.created_at DESC
        ) AS observations
    FROM base b
    GROUP BY b.classification, b.prompt_sha256, b.unknown_blocks
)
SELECT
    g.classification,
    g.prompt_sha256,
    g.unknown_blocks,
    COALESCE(sp.blocks->2->>'text', '') AS block_text,
    g.occurrence_count,
    g.first_seen,
    g.last_seen,
    g.observations,
    COALESCE(f.firing_rule_ids, '{}'::text[]) AS group_firing_rule_ids
FROM grouped g
LEFT JOIN firing f
    ON f.classification = g.classification
   AND f.prompt_sha256 IS NOT DISTINCT FROM g.prompt_sha256
   AND f.unknown_blocks IS NOT DISTINCT FROM g.unknown_blocks
LEFT JOIN system_prompts.system_prompts sp
    ON sp.prompt_sha256 = g.prompt_sha256
ORDER BY g.last_seen DESC
LIMIT 200 OFFSET 0;
