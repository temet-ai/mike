SELECT c.id,
       c.path,
       c.change,
       c.content_sha,
       c.seen_at,
       w.seats AS "seats: Vec<String>",
       w.named AS "named: Vec<String>"
  FROM launch.tree_changes c
  LEFT JOIN LATERAL (
        SELECT array_agg(DISTINCT l.seat) AS seats,
               array_agg(DISTINCT l.seat) FILTER (WHERE strpos(u.input::text, c.path) > 0) AS named
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
 WHERE c.run_id = $1
   AND c.seen_at >= $2
   AND c.path LIKE $3::text || '%'
 ORDER BY c.id DESC
 LIMIT $4
