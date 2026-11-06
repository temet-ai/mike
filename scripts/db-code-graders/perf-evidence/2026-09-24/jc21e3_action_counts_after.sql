SELECT count(*) AS total, count(*) FILTER (WHERE tu.operation IN ('read','search','fetch')) AS reads, count(*) FILTER (WHERE tu.operation = 'write') AS writes, count(*) FILTER (WHERE tu.tool_name = 'Bash') AS commands, count(*) FILTER (WHERE tu.tool_name = 'Bash' AND tr.is_error) AS failed_commands
  FROM tools.uses tu JOIN requests.requests r ON r.id = tu.request_id LEFT JOIN responses.stream_blocks sb ON sb.request_id = tu.request_id AND sb.wire_tool_use_id = tu.wire_tool_use_id
  LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id AND tr.created_at <= '2026-09-23 06:32:11.523098+00'
 WHERE tu.session_id = 1152749 AND COALESCE(sb.arrived_at, r.completed_at) <= '2026-09-23 06:32:11.523098+00';
