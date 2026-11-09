WITH handoffs AS (
               SELECT DISTINCT ON (l.thread_id) l.thread_id, l.entry_date
                 FROM work.thread_log l
                WHERE l.thread_id = ANY(ARRAY(SELECT ot.id FROM work.threads ot WHERE ot.status IS NULL OR ot.status NOT IN ('done', 'complete'))) AND l.kind = 'handoff'
                ORDER BY l.thread_id, l.ord DESC
           )
           SELECT r.thread_id AS "thread_id!", r.note_uuid AS "note_uuid!", r.kind AS "kind!",
                  r.title AS "title!", r.created_at AS "created_at!"
             FROM (
                   SELECT t.id AS thread_id, n.note_uuid, n.kind, n.title, n.created_at,
                          row_number() OVER (PARTITION BY t.id ORDER BY n.created_at DESC, n.id DESC) AS rn
                     FROM work.threads t
                     LEFT JOIN handoffs h ON h.thread_id = t.id
                     JOIN workspace.session_notes n ON n.tags @> ARRAY['thread:' || t.slug]
                     JOIN sessions.sessions se ON se.id = n.session_id AND se.project_id = t.project_id
                    WHERE t.id = ANY(ARRAY(SELECT ot.id FROM work.threads ot WHERE ot.status IS NULL OR ot.status NOT IN ('done', 'complete')))
                      AND n.kind IN ('decision', 'finding')
                      AND (h.entry_date IS NULL
                           OR n.created_at >= h.entry_date::timestamp AT TIME ZONE 'UTC')
                  ) r
            WHERE r.rn <= 2
            ORDER BY r.thread_id, r.created_at DESC
