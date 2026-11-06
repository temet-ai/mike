WITH working AS (
    SELECT s.id
      FROM sessions.sessions s
     WHERE s.cwd IS NOT NULL
       AND (s.cwd = '/home/vimes/worktrees/temet-proxy-change-record' OR starts_with(s.cwd, '/home/vimes/worktrees/temet-proxy-change-record' || '/'))
       AND s.last_seen >= '2026-10-06 06:03:55+00'::timestamptz - interval '1 hour'
)
SELECT u.session_id,
       coalesce(rs.completed_at, u.created_at) AS started,
       r.created_at AS finished
  FROM working w
  JOIN tools.uses u ON u.session_id = w.id
  LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
  LEFT JOIN tools.results r ON r.tool_use_id = u.id
 WHERE u.tool_name = 'Bash'
   AND u.created_at >= '2026-10-06 06:03:55+00'::timestamptz - interval '1 hour'
   AND u.created_at <= '2026-10-06 06:04:25+00'
   AND coalesce(rs.completed_at, u.created_at) <= '2026-10-06 06:04:25+00'
   AND (r.created_at IS NULL OR r.created_at >= '2026-10-06 06:03:55+00')
 ORDER BY 2, 1
