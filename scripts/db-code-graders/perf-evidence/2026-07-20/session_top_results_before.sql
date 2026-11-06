SELECT tu.tool_name,
       tu.request_id,
       tu.wire_tool_use_id AS tool_id,
       tr.content_bytes,
       LEFT(tu.input::text, 120) AS input_excerpt
  FROM tools.results tr
  JOIN tools.uses tu ON tu.id = tr.tool_use_id
  JOIN requests.requests r ON r.id = tu.request_id
 WHERE r.session_id = 100749
 ORDER BY tr.content_bytes DESC
 LIMIT 20;
