WITH open_threads AS (
    SELECT t.id, t.project_id, 'thread:' || t.slug AS tag
      FROM work.threads t
     WHERE t.status IS NULL OR t.status NOT IN ('done', 'complete')
),
session_times AS (
    SELECT o.id AS thread_id, GREATEST(max(p.max_at), max(v.max_at)) AS max_at
      FROM open_threads o
      JOIN work.thread_sessions ts ON ts.thread_id = o.id
      JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
      LEFT JOIN LATERAL (SELECT max(GREATEST(d.recorded_at, d.close_recorded_at)) AS max_at
                           FROM memory.dossier_entries d WHERE d.session_id = se.id AND d.kind = 'promise') p ON true
      LEFT JOIN LATERAL (SELECT v.created_at AS max_at FROM memory.turn_verdicts v
                          WHERE v.session_id = se.id AND v.mode = 'live' ORDER BY v.created_at DESC LIMIT 1) v ON true
     GROUP BY o.id
),
note_times AS (
    SELECT o.id AS thread_id, max(n.updated_at) AS max_at
      FROM open_threads o
      JOIN workspace.session_notes n ON n.tags @> ARRAY[o.tag] AND n.kind IN ('decision', 'finding')
      JOIN sessions.sessions se ON se.id = n.session_id AND se.project_id = o.project_id
     GROUP BY o.id
),
busy AS (
    SELECT st.thread_id, st.max_at FROM session_times st
    UNION ALL
    SELECT nt.thread_id, nt.max_at FROM note_times nt
)
SELECT b.thread_id, max(b.max_at) AS observed_at
  FROM busy b
 GROUP BY b.thread_id
HAVING max(b.max_at) > now() - interval '1 hour'
 ORDER BY b.thread_id
