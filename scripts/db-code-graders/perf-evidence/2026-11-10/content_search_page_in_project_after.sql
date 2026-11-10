WITH matches AS (
    SELECT
        t.session_id          AS session_pk,
        s.session_uuid::text  AS session_id,
        p.project_path,
        p.display_name        AS project_display_name,
        s.title,
        c.turn_id,
        t.start_request_id    AS request_id,
        cr.code               AS role,
        c.chunk_idx,
        c.chunk_text,
        t.created_at
    FROM memory.operator_turn_chunks c
    JOIN memory.operator_turns t ON t.id = c.turn_id
    JOIN memory.chunk_roles    cr ON cr.id = c.chunk_role_id
    JOIN sessions.sessions     s ON s.id = t.session_id
    JOIN projects.projects     p ON p.id = t.project_id
    WHERE c.chunk_text ILIKE ('%' || 'workflow' || '%')
      AND (t.opened_by IS NULL OR t.opened_by = 'operator')
      AND t.project_id = 993478
),
session_agg AS (
    SELECT
        session_pk, session_id, project_path, project_display_name, title,
        COUNT(*)                AS match_count,
        COUNT(*) FILTER (WHERE role = 'prompt') AS prompt_match_count,
        COUNT(DISTINCT turn_id) AS turn_count,
        MAX(created_at)         AS last_match_at
    FROM matches
    GROUP BY session_pk, session_id, project_path, project_display_name, title
),
paged_sessions AS (
    SELECT
        *,
        ROW_NUMBER() OVER (
            ORDER BY
                CASE WHEN 'matches'::text = 'matches' THEN match_count END DESC NULLS LAST,
                CASE WHEN 'matches'::text = 'matches' THEN prompt_match_count END DESC NULLS LAST,
                last_match_at DESC, session_id DESC
        ) AS session_rank
    FROM session_agg
    ORDER BY session_rank
    LIMIT 20 OFFSET 0
)
SELECT
    ps.session_id AS "session_id!", ps.project_path AS "project_path!",
    ps.project_display_name AS "project_display_name!",
    ps.title, ps.match_count AS "match_count!",
    ps.prompt_match_count AS "prompt_match_count!", ps.turn_count AS "turn_count!",
    ps.last_match_at AS "last_match_at!",
    m.turn_id AS "turn_id!", m.request_id AS "request_id!", m.role AS "role!",
    m.chunk_idx AS "chunk_idx!", m.chunk_text AS "chunk_text!", m.created_at AS "created_at!"
FROM paged_sessions ps
JOIN matches m ON m.session_pk = ps.session_pk
ORDER BY ps.session_rank, m.created_at DESC, m.chunk_idx ASC
