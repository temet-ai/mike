SELECT u.id AS use_id,
       l.seat AS seat,
       l.session_uuid AS session,
       rs.completed_at AS started_at,
       r.created_at AS finished_at,
       r.is_error,
       coalesce(u.input->>'run_in_background' = 'true', false) AS in_background,
       u.input->>'command' AS command
  FROM launch.launches l
  JOIN sessions.sessions s    ON s.session_uuid = l.session_uuid
  JOIN tools.uses u           ON u.session_id = s.id AND u.tool_name = 'Bash'
  JOIN responses.responses rs ON rs.request_id = u.request_id
  JOIN tools.results r        ON r.tool_use_id = u.id
 WHERE l.run_id = 'fleet-judgement-at-work-541ce7e3'
   AND l.seat IS NOT NULL
   AND l.seat NOT IN ('coordinator', 'observer', 'status')
   AND u.created_at >= '2026-09-28 19:00:00+00'
   AND (r.created_at - rs.completed_at >= make_interval(secs => 20.0::float8)
        OR u.input->>'run_in_background' = 'true')
   AND u.input->>'command' IS NOT NULL
 ORDER BY rs.completed_at, u.id
