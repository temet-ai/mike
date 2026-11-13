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
    WHERE c.chunk_text ILIKE ('%' || $1 || '%')
      AND (t.opened_by IS NULL OR t.opened_by = 'operator')
      AND t.project_id = $2
),
session_agg AS (
    SELECT
        session_pk, session_id, project_path, project_display_name, title,
        COUNT(*)                AS match_count,
        COUNT(DISTINCT turn_id) AS turn_count,
        MAX(created_at)         AS last_match_at
    FROM matches
    GROUP BY session_pk, session_id, project_path, project_display_name, title
)
SELECT
    COALESCE(SUM(match_count), 0)::bigint AS "total_matches!",
    COUNT(*)::bigint                       AS "total_sessions!"
FROM session_agg
