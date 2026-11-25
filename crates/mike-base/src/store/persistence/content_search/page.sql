WITH matches AS MATERIALIZED (
    SELECT
        c.id AS chunk_id, c.turn_id, c.chunk_idx, c.chunk_role_id,
        t.session_id AS session_pk, t.project_id, t.start_request_id AS request_id, t.created_at,
          CASE WHEN f.folded LIKE ($1::text[])[1] THEN   1 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[2] THEN   2 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[3] THEN   4 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[4] THEN   8 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[5] THEN  16 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[6] THEN  32 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[7] THEN  64 ELSE 0 END
        | CASE WHEN f.folded LIKE ($1::text[])[8] THEN 128 ELSE 0 END AS words
    FROM memory.operator_turn_chunks c
    JOIN memory.operator_turns t ON t.id = c.turn_id
    CROSS JOIN LATERAL (SELECT lower(c.chunk_text COLLATE "pg_c_utf8") AS folded OFFSET 0) f
    WHERE ($3::bool OR lower(c.chunk_text COLLATE "pg_c_utf8") LIKE ANY ($1::text[]))
      AND (t.opened_by IS NULL OR t.opened_by = 'operator')
      AND ($4::bigint IS NULL OR t.project_id = $4)
),
turn_agg AS (
    SELECT
        m.session_pk, m.project_id, m.turn_id, m.created_at,
        bit_or(m.words) AS words,
        COUNT(*) AS match_count,
        COUNT(*) FILTER (WHERE cr.code = 'prompt') AS prompt_match_count
    FROM matches m
    JOIN memory.chunk_roles cr ON cr.id = m.chunk_role_id
    WHERE m.words <> 0
    GROUP BY m.session_pk, m.project_id, m.turn_id, m.created_at
),
session_agg AS (
    SELECT
        ta.session_pk, ta.project_id,
        bit_count(bit_or(bit_or(ta.words)) OVER (PARTITION BY ta.session_pk)::bit(8)) AS words_matched,
        SUM(ta.match_count)::bigint        AS match_count,
        SUM(ta.prompt_match_count)::bigint AS prompt_match_count,
        COUNT(*)                           AS turn_count,
        MAX(ta.created_at)                 AS last_match_at
    FROM turn_agg ta
    GROUP BY ta.session_pk, ta.project_id
),
paged_sessions AS (
    SELECT
        sa.session_pk, sa.project_id, sa.words_matched, sa.match_count, sa.prompt_match_count,
        sa.turn_count, sa.last_match_at, s.session_uuid::text AS session_id, s.title,
        SUM(sa.match_count) OVER ()::bigint AS total_matches,
        COUNT(*) OVER ()                    AS total_sessions,
        ROW_NUMBER() OVER (
            ORDER BY
                sa.words_matched DESC,
                CASE WHEN $7::text = 'matches' THEN sa.match_count END DESC NULLS LAST,
                CASE WHEN $7::text = 'matches' THEN sa.prompt_match_count END DESC NULLS LAST,
                sa.last_match_at DESC, s.session_uuid::text DESC
        ) AS session_rank
    FROM session_agg sa
    JOIN sessions.sessions s ON s.id = sa.session_pk
    WHERE sa.words_matched >= $2::int
    ORDER BY session_rank
    LIMIT $5 OFFSET $6
),
shown AS (
    SELECT
        ps.session_rank, m.chunk_id, m.turn_id, m.chunk_idx, m.chunk_role_id, m.request_id, m.created_at,
        ROW_NUMBER() OVER (
            PARTITION BY ps.session_rank ORDER BY m.created_at DESC, m.chunk_idx ASC, m.chunk_id ASC
        ) AS match_rank
    FROM paged_sessions ps
    JOIN matches m ON m.session_pk = ps.session_pk
    WHERE m.words <> 0
)
SELECT
    ps.total_matches AS "total_matches!", ps.total_sessions AS "total_sessions!",
    ps.session_id AS "session_id!", p.project_path AS "project_path!",
    p.display_name AS "project_display_name!",
    ps.title AS "title?", ps.words_matched AS "words_matched!", ps.match_count AS "match_count!",
    ps.prompt_match_count AS "prompt_match_count!", ps.turn_count AS "turn_count!",
    ps.last_match_at AS "last_match_at!",
    sh.turn_id AS "turn_id!", sh.request_id AS "request_id!", cr.code AS "role!",
    sh.chunk_idx AS "chunk_idx!", c.chunk_text AS "chunk_text!", sh.created_at AS "created_at!"
FROM shown sh
JOIN paged_sessions ps ON ps.session_rank = sh.session_rank
JOIN projects.projects p ON p.id = ps.project_id
JOIN memory.chunk_roles cr ON cr.id = sh.chunk_role_id
JOIN memory.operator_turn_chunks c ON c.id = sh.chunk_id
WHERE $8::bigint IS NULL OR sh.match_rank <= $8
ORDER BY sh.session_rank, sh.match_rank
