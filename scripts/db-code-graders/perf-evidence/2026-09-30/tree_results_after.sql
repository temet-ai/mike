SELECT u.id AS use_id,
       l.seat,
       l.session_uuid,
       u.input->>'command' AS command,
       rs.completed_at AS started_at,
       r.created_at AS finished_at,
       r.is_error,
       r.content
  FROM launch.launches l
  JOIN sessions.sessions s    ON s.session_uuid = l.session_uuid
  JOIN tools.uses u           ON u.session_id = s.id AND u.tool_name = 'Bash'
  JOIN responses.responses rs ON rs.request_id = u.request_id
  LEFT JOIN tools.results r   ON r.tool_use_id = u.id
 WHERE l.run_id = 'fleet-x-1'
   AND l.seat IS NOT NULL
   AND u.created_at >= '2026-09-30 00:00+00'
   AND (NULL::text IS NULL OR (u.input->>'command') ILIKE '%' || NULL || '%')
 ORDER BY u.id DESC
 LIMIT 20
