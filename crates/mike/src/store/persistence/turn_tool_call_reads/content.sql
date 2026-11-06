WITH turn_use AS (
    SELECT tu.id, tu.input
      FROM tools.uses tu
      JOIN requests.requests r ON r.id = tu.request_id
     WHERE r.operator_turn_id = $1
       AND tu.session_id = $2
       AND tu.wire_tool_use_id = $3
     LIMIT 1
)
SELECT tu.input                AS "input?",
       res.is_error            AS "result_is_error?",
       res.content             AS "result_content?",
       res.content_bytes       AS "result_content_bytes?",
       res.content_sha256      AS "result_content_sha256?"
  FROM turn_use tu
  LEFT JOIN tools.results res ON res.tool_use_id = tu.id
