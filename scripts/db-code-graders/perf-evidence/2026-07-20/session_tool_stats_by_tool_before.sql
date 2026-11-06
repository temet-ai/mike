SELECT tu.tool_name,
       COUNT(*)::bigint AS use_count,
       COALESCE(SUM(tr.content_bytes), 0)::bigint AS total_result_bytes
  FROM tools.uses tu
  JOIN requests.requests r ON r.id = tu.request_id
  LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
 WHERE r.session_id = 100749
 GROUP BY tu.tool_name
 ORDER BY total_result_bytes DESC, use_count DESC;
