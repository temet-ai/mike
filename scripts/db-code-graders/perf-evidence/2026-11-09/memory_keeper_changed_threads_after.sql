SELECT t.id AS thread_id, GREATEST(t.updated_at, s.updated_at, l.max_at, ts.max_at) AS observed_at
  FROM work.threads t
  LEFT JOIN work.thread_state s ON s.thread_id = t.id
  LEFT JOIN LATERAL (SELECT max(lg.created_at) AS max_at FROM work.thread_log lg WHERE lg.thread_id = t.id) l ON true
  LEFT JOIN LATERAL (SELECT max(x.first_seen_at) AS max_at FROM work.thread_sessions x WHERE x.thread_id = t.id) ts ON true
 WHERE (t.status IS NULL OR t.status NOT IN ('done', 'complete'))
 ORDER BY t.id
