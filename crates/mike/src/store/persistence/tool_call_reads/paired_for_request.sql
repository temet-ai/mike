SELECT tu.wire_tool_use_id AS "tool_id!",
       tu.tool_name AS "tool_name!",
       tu.input,
       tu.ord AS "ord!",
       CASE WHEN tr.id IS NOT NULL THEN tu.wire_tool_use_id END AS "result_tool_use_id?",
       tr.is_error      AS "result_is_error?",
       tr.content_sha256 AS "result_content_sha256?",
       tr.content_bytes AS "result_content_bytes?",
       tr.content       AS "result_content?"
  FROM tools.uses tu
  LEFT JOIN tools.results tr
         ON tr.tool_use_id = tu.id
 WHERE tu.request_id = $1
 ORDER BY tu.ord ASC
