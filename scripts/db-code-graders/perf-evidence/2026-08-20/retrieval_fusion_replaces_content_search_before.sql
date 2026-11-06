-- BEFORE: one user search on /sessions today, cheapest of the three round trips
-- it fires — GET /api/content/search (store/persistence/content_search.rs
-- MATCH_CTE + search_content_page), corpus-wide, exactly as the SPA calls it.
WITH matches AS (
    SELECT t.session_id AS session_pk, s.session_uuid::text AS session_id,
           p.project_path, p.display_name AS project_display_name, s.title,
           c.turn_id, t.start_request_id AS request_id, cr.code AS role,
           c.chunk_idx, c.chunk_text, t.created_at
    FROM memory.operator_turn_chunks c
    JOIN memory.operator_turns t ON t.id = c.turn_id
    JOIN memory.chunk_roles cr ON cr.id = c.chunk_role_id
    JOIN sessions.sessions s ON s.id = t.session_id
    JOIN projects.projects p ON p.id = t.project_id
    WHERE c.chunk_text ILIKE ('%' || 'embedding' || '%')
      AND (NULL::text IS NULL OR p.project_path = NULL)
),
session_agg AS (
    SELECT session_pk, session_id, project_path, project_display_name, title,
           COUNT(*) AS match_count, COUNT(DISTINCT turn_id) AS turn_count,
           MAX(created_at) AS last_match_at
    FROM matches
    GROUP BY session_pk, session_id, project_path, project_display_name, title
),
paged_sessions AS (
    SELECT * FROM session_agg ORDER BY last_match_at DESC, session_id DESC LIMIT 20 OFFSET 0
)
SELECT ps.session_id, ps.project_path, ps.project_display_name, ps.title,
       ps.match_count, ps.turn_count, ps.last_match_at,
       m.turn_id, m.request_id, m.role, m.chunk_idx, m.chunk_text, m.created_at
FROM paged_sessions ps
JOIN matches m ON m.session_pk = ps.session_pk
ORDER BY ps.last_match_at DESC, ps.session_id DESC, m.created_at DESC, m.chunk_idx ASC;
