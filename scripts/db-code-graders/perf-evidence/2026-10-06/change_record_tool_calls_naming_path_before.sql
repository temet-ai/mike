SELECT u.session_id,
       u.id AS tool_use_id,
       u.tool_name,
       u.created_at AS at,
       coalesce(rs.completed_at, u.created_at) AS "ran_from!",
       r.created_at AS "ran_to?",
       u.input
  FROM tools.uses u
  LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
  LEFT JOIN tools.results r ON r.tool_use_id = u.id
 WHERE u.file_path = '/home/vimes/worktrees/temet-proxy-change-record/docs/SCHEMA.md'
   AND u.created_at >= '2026-10-05 19:35:30+00'
   AND u.created_at <= '2026-10-05 19:36:02+00'
   AND u.tool_name IN ('Edit', 'Write', 'MultiEdit')
 ORDER BY u.created_at, u.id
