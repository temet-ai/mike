WITH wanted AS (
               SELECT c.thread_id, c.ref, c.kind, c.score, t.project_id,
                      substr(c.ref, position(':' IN c.ref) + 1) AS key
                 FROM work.thread_card_candidates c
                 JOIN work.threads t ON t.id = c.thread_id
                WHERE c.thread_id = ANY(ARRAY[2,4,6,8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48,50,52,54,56,58,60,62,64,66,68,70,72,74,76,78,80,82,84,86,88,90,92,94,96,98,100]::bigint[])
           ), live AS (
               SELECT w.thread_id, w.ref, w.score,
                      btrim(split_part(btrim(l.text), E'\n', 1)) AS text
                 FROM wanted w
                 JOIN memory.lessons l
                   ON l.uuid = CASE WHEN w.key ~ '^[0-9a-fA-F-]{36}$' THEN w.key::uuid END
                  AND l.project_id = w.project_id
                 JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                 JOIN memory.lesson_sources src ON src.id = l.source_id
                 LEFT JOIN memory.lesson_shape_findings open_shape
                   ON open_shape.lesson_id = l.id AND open_shape.resolution IS NULL
                WHERE w.kind = 'lesson'
                  AND (ls.code IN ('daily', 'long_term') OR (ls.code = 'raw' AND src.code = 'operator'))
                  AND (l.expires_at IS NULL OR l.expires_at > now())
                  AND NOT EXISTS (
                      SELECT 1
                        FROM memory.lesson_relations r
                        JOIN memory.lesson_relation_kinds rk
                          ON rk.id = r.kind_id AND rk.code = 'supersedes'
                        JOIN memory.lesson_relation_statuses rs
                          ON rs.id = r.status_id AND rs.code = 'confirmed'
                       WHERE r.to_lesson_id = l.id
                  )
                  AND (ls.code <> 'raw' OR open_shape.lesson_id IS NULL)
               UNION ALL
               SELECT w.thread_id, w.ref, w.score, btrim(n.title)
                 FROM wanted w
                 JOIN workspace.session_notes n
                   ON n.note_uuid = CASE WHEN w.key ~ '^[0-9a-fA-F-]{36}$' THEN w.key::uuid END
                WHERE w.kind = 'note'
                  AND COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected')
               UNION ALL
               SELECT w.thread_id, w.ref, w.score,
                      COALESCE(NULLIF(btrim(split_part(btrim(g.summary), E'\n', 1)), ''), g.kind)
                 FROM wanted w
                 JOIN work.thread_log g
                   ON g.id = CASE WHEN w.key ~ '^[0-9]{1,18}$' THEN w.key::bigint END
                WHERE w.kind = 'entry'
               UNION ALL
               SELECT w.thread_id, w.ref, w.score,
                      btrim(split_part(btrim(COALESCE(tx.user_prompt, '')), E'\n', 1))
                 FROM wanted w
                 JOIN memory.operator_turns ot
                   ON ot.id = CASE WHEN w.key ~ '^[0-9]{1,18}$' THEN w.key::bigint END
                 JOIN mike.operator_turn_text_v tx ON tx.turn_id = ot.id
                WHERE w.kind = 'turn'
           ), ranked AS (
               SELECT live.thread_id, live.ref, live.score, live.text,
                      row_number() OVER (PARTITION BY live.thread_id
                                         ORDER BY live.score DESC, live.ref) AS rn
                 FROM live
           )
           SELECT r.thread_id AS "thread_id!", r.ref AS "reference!", r.score AS "score!",
                  COALESCE(r.text, '') AS "text!"
             FROM ranked r
            WHERE r.rn <= 3
            ORDER BY r.thread_id, r.score DESC, r.ref;
