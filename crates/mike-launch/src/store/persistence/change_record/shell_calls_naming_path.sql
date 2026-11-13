WITH working AS (
    SELECT s.id
      FROM sessions.sessions s
     WHERE s.cwd IS NOT NULL
       AND (s.cwd = $1 OR starts_with(s.cwd, $1 || '/'))
       AND s.last_seen >= $4::timestamptz - interval '1 hour'
)
SELECT u.session_id,
       u.id AS tool_use_id,
       coalesce(rs.completed_at, u.created_at) AS "started!",
       r.created_at AS "finished?"
  FROM working w
  JOIN tools.uses u ON u.session_id = w.id
  LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
  LEFT JOIN tools.results r ON r.tool_use_id = u.id
 WHERE u.tool_name = 'Bash'
   AND u.created_at >= $4::timestamptz - interval '1 hour'
   AND u.created_at <= $5::timestamptz + interval '1 hour'
   AND (strpos(u.input->>'command', $2) > 0 OR strpos(u.input->>'command', $3) > 0)
   AND coalesce(rs.completed_at, u.created_at) <= $5::timestamptz + interval '1 second'
   AND (r.created_at IS NULL OR r.created_at >= $4::timestamptz - interval '1 second')
   AND r.is_error IS NOT TRUE
 ORDER BY 3, 1
