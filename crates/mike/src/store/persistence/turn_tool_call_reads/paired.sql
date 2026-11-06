WITH turn_uses AS (
    SELECT tu.id, tu.wire_tool_use_id, tu.tool_name,
           tu.input, tu.ord, tu.request_id, tu.operation
      FROM tools.uses tu
      JOIN requests.requests r ON r.id = tu.request_id
     WHERE r.operator_turn_id = $1
       AND tu.session_id = $2
)
SELECT u.wire_tool_use_id               AS "tool_id!",
       u.tool_name                      AS "tool_name!",
       octet_length(u.input::text)::int AS "input_bytes?",
       u.ord                            AS "ord!",
       u.request_id                     AS "request_id!",
       (res.tool_use_id IS NOT NULL)    AS "has_result!",
       res.is_error                     AS "result_is_error?",
       res.content_bytes                AS "result_content_bytes?",
       u.operation                      AS "operation?"
  FROM turn_uses u
  LEFT JOIN tools.results res ON res.tool_use_id = u.id
 ORDER BY u.request_id ASC, u.ord ASC
