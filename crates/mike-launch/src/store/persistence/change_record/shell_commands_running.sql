WITH working AS (
    SELECT s.id
      FROM sessions.sessions s
     WHERE s.cwd IS NOT NULL
       AND (s.cwd = $1 OR starts_with(s.cwd, $1 || '/'))
       AND s.last_seen >= $2::timestamptz - interval '1 hour'
),
calls AS (
    SELECT u.id,
           u.session_id,
           coalesce(rs.completed_at, u.created_at) AS started,
           r.created_at AS finished,
           least(
               CASE WHEN u.input->>'timeout' ~ '^[0-9]{1,9}$'
                    THEN (u.input->>'timeout')::bigint
                    ELSE 120000 END,
               600000
           ) AS timeout_ms
      FROM working w
      JOIN tools.uses u ON u.session_id = w.id
      LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
      LEFT JOIN tools.results r ON r.tool_use_id = u.id
     WHERE u.tool_name = 'Bash'
       AND u.created_at >= $2::timestamptz - interval '1 hour'
       AND u.created_at <= $3
       AND coalesce(rs.completed_at, u.created_at) <= $3
       AND (r.created_at IS NULL OR r.created_at >= $2)
),
overdue AS (
    SELECT c.id, c.session_id, c.started
      FROM calls c
     WHERE c.finished IS NULL
       AND c.started + c.timeout_ms * interval '1 millisecond' < $3
),
alive AS (
    SELECT DISTINCT o.id
      FROM overdue o
      JOIN requests.requests q ON q.session_id = o.session_id
     WHERE q.created_at > o.started
       AND q.created_at >= $3::timestamptz - interval '5 minutes'
       AND q.created_at <= $3
)
SELECT c.session_id,
       c.started AS "started!",
       c.finished AS "finished?"
  FROM calls c
  LEFT JOIN overdue o ON o.id = c.id
  LEFT JOIN alive a ON a.id = c.id
 WHERE o.id IS NULL
    OR a.id IS NOT NULL
 ORDER BY 2, 1
