SELECT tr.tool_use_id AS "tu_id!",
       tr.request_id AS "request_id!",
       tu.wire_tool_use_id AS "wire_tool_use_id!",
       tr.is_error AS "is_error!",
       tr.content_sha256 AS "content_sha256!",
       tr.content_bytes AS "content_bytes!",
       tr.content AS "content!"
  FROM tools.results tr
  JOIN tools.uses tu
       ON tu.id = tr.tool_use_id
      AND tu.id = ANY($1)
 WHERE tr.tool_use_id = ANY($1)
   AND tr.session_id = $2
