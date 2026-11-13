SELECT tu.wire_tool_use_id AS "tool_use_id!", tr.is_error AS "is_error!", tr.content_sha256 AS "content_sha256!",
       tr.content_bytes AS "content_bytes!", tr.content AS "content!"
  FROM tools.results tr
  JOIN tools.uses    tu ON tu.id = tr.tool_use_id
 WHERE tu.request_id = $1
 ORDER BY tr.created_at ASC, tr.ord ASC
