SELECT DISTINCT elem ->> 'kind' AS "kind!"
  FROM tools.uses tu
  CROSS JOIN LATERAL jsonb_array_elements(tu.input -> 'context') AS elem
 WHERE tu.tool_name ILIKE '%send_session_message%'
   AND jsonb_typeof(tu.input -> 'context') = 'array'
