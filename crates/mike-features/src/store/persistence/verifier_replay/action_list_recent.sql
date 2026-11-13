SELECT x.tool_name AS "tool_name!", x.file_path, x.command AS "command?",
       x.has_result AS "has_result!", x.result_is_error AS "result_is_error?", x.operation
  FROM (
       SELECT tu.tool_name,
              tu.file_path,
              CASE WHEN tu.tool_name = 'Bash'
                   THEN tu.input ->> 'command'
              END AS command,
              (tr.tool_use_id IS NOT NULL) AS has_result,
              tr.is_error AS result_is_error,
              tu.operation,
              tu.created_at,
              tu.ord
         FROM tools.uses tu
         JOIN requests.requests r ON r.id = tu.request_id
         LEFT JOIN responses.stream_blocks sb
                ON sb.request_id = tu.request_id
               AND sb.wire_tool_use_id = tu.wire_tool_use_id
         LEFT JOIN tools.results tr
                ON tr.tool_use_id = tu.id AND tr.created_at <= $2
        WHERE tu.session_id = $1
          AND COALESCE(sb.arrived_at, r.completed_at) <= $2
        ORDER BY tu.created_at DESC, tu.ord DESC
        LIMIT $3) x
 ORDER BY x.created_at ASC, x.ord ASC
