WITH calls AS (
    SELECT u.id,
           u.session_id,
           coalesce(b0.arrived_at, rs.completed_at, u.created_at) AS started,
           CASE WHEN b.backgrounded
                THEN (SELECT min(q.created_at)
                        FROM requests.requests q
                       WHERE q.session_id = u.session_id
                         AND q.created_at > r.created_at
                         AND (strpos(q.last_user_message, '<tool-use-id>' || u.wire_tool_use_id) > 0
                              OR strpos(q.last_user_message, '<task-id>' || b.task || '</task-id>') > 0))
                ELSE r.created_at
            END AS finished,
           b.backgrounded,
           u.input->>'command' AS command,
           least(
               CASE WHEN u.input->>'timeout' ~ '^[0-9]{1,9}$'
                    THEN (u.input->>'timeout')::bigint
                    ELSE 120000 END,
               600000
           ) AS timeout_ms,
           coalesce(strpos(replace(u.input->>'command', $1 || '/.claude/worktrees/', ''), $1) > 0
            OR (starts_with($1, s.cwd || '/')
                AND strpos(u.input->>'command', substr($1, length(s.cwd) + 2)) > 0), false) AS names_checkout
      FROM tools.uses u
      JOIN sessions.sessions s ON s.id = u.session_id
      LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
      LEFT JOIN responses.stream_blocks b0 ON b0.request_id = u.request_id AND b0.wire_tool_use_id = u.wire_tool_use_id
      LEFT JOIN tools.results r ON r.tool_use_id = u.id
      CROSS JOIN LATERAL (
          SELECT t.task, t.task IS NOT NULL AS backgrounded
            FROM (SELECT substring(left(r.content::text, 300)
                                   FROM '^"Command [^\n]{0,80}?background (?:with ID: |\(ID: )([a-z0-9]+)') AS task) t
      ) b
     WHERE u.tool_name = 'Bash'
       AND u.created_at >= $2::timestamptz - interval '1 hour'
       AND u.created_at <= $3
       AND s.cwd IS NOT NULL
       AND (s.cwd = $1
            OR starts_with(s.cwd, $1 || '/')
            OR strpos(u.input->>'command', $1) > 0
            OR (starts_with($1, s.cwd || '/')
                AND strpos(u.input->>'command', substr($1, length(s.cwd) + 2)) > 0))
       AND coalesce(b0.arrived_at, rs.completed_at, u.created_at) <= $3
       AND (r.created_at IS NULL OR r.created_at >= $2 OR b.backgrounded)
),
overdue AS (
    SELECT c.id, c.session_id, c.started
      FROM calls c
     WHERE c.finished IS NULL
       AND NOT c.backgrounded
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
       c.finished AS "finished?",
       c.names_checkout AS "names_checkout!",
       c.command AS "command?"
  FROM calls c
  LEFT JOIN overdue o ON o.id = c.id
  LEFT JOIN alive a ON a.id = c.id
 WHERE (o.id IS NULL OR a.id IS NOT NULL)
   AND (c.finished IS NULL OR c.finished >= $2)
   AND (c.finished IS NOT NULL OR NOT c.backgrounded OR c.names_checkout)
 ORDER BY 2, 1
