SELECT c.id,
       c.path,
       c.change,
       c.content_sha,
       c.seen_at,
       w.seats AS seats
  FROM launch.tree_changes c
  LEFT JOIN LATERAL (
        SELECT array_agg(DISTINCT l.seat) AS seats
          FROM launch.launches l
          JOIN sessions.sessions s   ON s.session_uuid = l.session_uuid
          JOIN tools.uses u          ON u.session_id = s.id
          JOIN responses.responses rs ON rs.request_id = u.request_id
          LEFT JOIN tools.results r  ON r.tool_use_id = u.id
         WHERE l.run_id = c.run_id
           AND l.seat IS NOT NULL
           AND u.tool_name IN ('Bash', 'Edit', 'Write', 'MultiEdit', 'NotebookEdit')
           AND u.created_at BETWEEN c.seen_at - interval '30 minutes' AND c.seen_at + interval '1 second'
           AND rs.completed_at <= c.seen_at + interval '1 second'
           AND (r.created_at IS NULL OR r.created_at >= c.seen_at - interval '1 second')
       ) w ON true
 WHERE c.run_id = 'fleet-x-1'
   AND c.seen_at >= '2026-09-30 00:00+00'
   AND (NULL::text IS NULL OR c.path LIKE NULL || '%')
 ORDER BY c.id DESC
 LIMIT 100
