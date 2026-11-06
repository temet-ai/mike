WITH structural AS (
    SELECT
        r.id AS request_id,
        sess.session_uuid::text AS session_id,
        r.created_at,
        NULL::text AS prompt_sha256,
        'structural-mismatch'::text AS classification,
        rsp.unknown_blocks,
        COALESCE(fru.firing_rule_ids, '{}'::text[]) AS firing_rule_ids
    FROM requests.requests r
    JOIN sessions.sessions sess
        ON sess.id = r.session_id
    JOIN requests.system_prompts rsp
        ON rsp.request_id = r.id
    LEFT JOIN LATERAL (
        SELECT ARRAY_AGG(fr.uuid::text ORDER BY j.ord) AS firing_rule_ids
        FROM requests.system_prompt_firing_rules j
        JOIN rules.rules fr ON fr.id = j.filter_rule_id
        WHERE j.request_system_prompt_id = rsp.id
    ) fru ON true
    WHERE rsp.unknown_blocks IS NOT NULL
      AND r.created_at >= COALESCE(NULL::timestamptz, NOW() - INTERVAL '7 days')
      AND r.created_at < COALESCE(NULL::timestamptz, NOW())
),
unknown_class AS (
    SELECT
        r.id AS request_id,
        sess.session_uuid::text AS session_id,
        r.created_at,
        rsp.prompt_sha256 AS prompt_sha256,
        'unknown-class'::text AS classification,
        NULL::jsonb AS unknown_blocks,
        COALESCE(fru.firing_rule_ids, '{}'::text[]) AS firing_rule_ids
    FROM requests.requests r
    JOIN sessions.sessions sess
        ON sess.id = r.session_id
    JOIN requests.system_prompts rsp
        ON rsp.request_id = r.id
    LEFT JOIN LATERAL (
        SELECT ARRAY_AGG(fr.uuid::text ORDER BY j.ord) AS firing_rule_ids
        FROM requests.system_prompt_firing_rules j
        JOIN rules.rules fr ON fr.id = j.filter_rule_id
        WHERE j.request_system_prompt_id = rsp.id
    ) fru ON true
    WHERE rsp.prompt_sha256 IS NOT NULL
      AND r.created_at >= COALESCE(NULL::timestamptz, NOW() - INTERVAL '7 days')
      AND r.created_at < COALESCE(NULL::timestamptz, NOW())
      AND NOT EXISTS (
          SELECT 1 FROM overrides.overrides pc
          JOIN overrides.sources src ON src.id = pc.source_id AND src.code = 'manual'
          WHERE pc.prompt_sha256 = rsp.prompt_sha256
      )
),
combined AS (
    SELECT request_id, session_id, created_at, prompt_sha256,
           classification, unknown_blocks,
           firing_rule_ids
    FROM structural
    WHERE NOT EXISTS (
        SELECT 1 FROM overrides.overrides pc
        JOIN overrides.sources src ON src.id = pc.source_id AND src.code = 'filter'
        JOIN rules.rules fr ON fr.id = pc.filter_rule_id
        WHERE fr.uuid::text = ANY(structural.firing_rule_ids)
    )
    UNION ALL
    SELECT request_id, session_id, created_at, prompt_sha256,
           classification, unknown_blocks,
           firing_rule_ids
    FROM unknown_class
),
firing_rule_aggs AS (
    SELECT
        classification,
        prompt_sha256,
        unknown_blocks,
        ARRAY_AGG(DISTINCT rid) AS rule_ids
    FROM combined,
         unnest(firing_rule_ids) AS rid
    GROUP BY classification, prompt_sha256, unknown_blocks
),
grouped AS (
    SELECT
        c.classification,
        c.prompt_sha256,
        c.unknown_blocks,
        COUNT(*)::bigint AS occurrence_count,
        MIN(c.created_at) AS first_seen,
        MAX(c.created_at) AS last_seen,
        ARRAY_AGG(c.request_id ORDER BY c.created_at DESC) AS request_ids,
        ARRAY_AGG(DISTINCT c.session_id) AS session_ids,
        COALESCE(MAX(fra.rule_ids)::TEXT[], ARRAY[]::TEXT[]) AS group_firing_rule_ids
    FROM combined c
    LEFT JOIN firing_rule_aggs fra
        ON fra.classification = c.classification
       AND fra.prompt_sha256 IS NOT DISTINCT FROM c.prompt_sha256
       AND fra.unknown_blocks IS NOT DISTINCT FROM c.unknown_blocks
    GROUP BY c.classification, c.prompt_sha256, c.unknown_blocks
)
SELECT
    g.classification,
    g.prompt_sha256,
    g.unknown_blocks,
    COALESCE(sp.blocks->2->>'text', '') AS block_text,
    g.occurrence_count,
    g.first_seen,
    g.last_seen,
    g.request_ids,
    g.session_ids,
    g.group_firing_rule_ids
FROM grouped g
LEFT JOIN system_prompts.system_prompts sp
    ON sp.prompt_sha256 = g.prompt_sha256
ORDER BY g.last_seen DESC
LIMIT 100 OFFSET 0
