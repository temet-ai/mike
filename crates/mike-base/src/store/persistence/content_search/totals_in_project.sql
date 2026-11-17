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
    WHERE c.chunk_text ILIKE ANY ($1::text[])
      AND (t.opened_by IS NULL OR t.opened_by = 'operator')
      AND t.project_id = $3
),
word_hits AS (
    SELECT m.session_pk, COUNT(DISTINCT w.ord) AS words_matched
    FROM matches m
    CROSS JOIN LATERAL unnest($1::text[]) WITH ORDINALITY AS w(pattern, ord)
    WHERE m.chunk_text ILIKE w.pattern
    GROUP BY m.session_pk
),
session_agg AS (
    SELECT
        m.session_pk, m.session_id, m.project_path, m.project_display_name, m.title,
        COUNT(*)                AS match_count,
        COUNT(DISTINCT m.turn_id) AS turn_count,
        MAX(m.created_at)       AS last_match_at
    FROM matches m
    JOIN word_hits wh ON wh.session_pk = m.session_pk AND wh.words_matched >= $2::int
    GROUP BY m.session_pk, m.session_id, m.project_path, m.project_display_name, m.title
)
SELECT
    COALESCE(SUM(match_count), 0)::bigint AS "total_matches!",
    COUNT(*)::bigint                       AS "total_sessions!"
FROM session_agg
