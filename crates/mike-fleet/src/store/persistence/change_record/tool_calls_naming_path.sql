SELECT u.session_id,
       u.id AS tool_use_id,
       u.tool_name,
       u.created_at AS at,
       coalesce(b0.arrived_at, rs.completed_at, u.created_at) AS "ran_from!",
       r.created_at AS "ran_to?",
       r.is_error AS "failed?",
       u.input
  FROM tools.uses u
  LEFT JOIN responses.responses rs ON rs.request_id = u.request_id
  LEFT JOIN responses.stream_blocks b0 ON b0.request_id = u.request_id AND b0.wire_tool_use_id = u.wire_tool_use_id
  LEFT JOIN tools.results r ON r.tool_use_id = u.id
 WHERE u.file_path = $1
   AND u.created_at >= $2
   AND u.created_at <= $3
   AND u.tool_name IN ('Edit', 'Write', 'MultiEdit')
 ORDER BY u.created_at, u.id
