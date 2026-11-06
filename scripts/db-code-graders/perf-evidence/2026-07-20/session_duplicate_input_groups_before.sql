SELECT tu.tool_name,
       md5(tu.input::text) AS input_digest,
       COUNT(*)::bigint AS use_count,
       COALESCE(AVG(tr.content_bytes), 0)::float8 AS avg_result_bytes,
       (ARRAY_AGG(tu.request_id ORDER BY tu.request_id))[1:6] AS sample_request_ids,
       LEFT(MIN(tu.input::text), 120) AS input_excerpt
  FROM tools.uses tu
  JOIN requests.requests r ON r.id = tu.request_id
  LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
 WHERE r.session_id = 100749
   AND tu.input IS NOT NULL
 GROUP BY tu.tool_name, md5(tu.input::text)
HAVING COUNT(*) > 1
 ORDER BY use_count DESC, avg_result_bytes DESC;
