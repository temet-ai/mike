SELECT tu.wire_tool_use_id,
        left(COALESCE(tu.input->>'command', tu.input->>'file_path', tu.input->>'notebook_path',
                      tu.input->>'path', tu.input->>'pattern', tu.input->>'url'), 600),
        left(CASE jsonb_typeof(res.content)
                  WHEN 'string' THEN res.content #>> '{}'
                  WHEN 'array' THEN res.content -> 0 ->> 'text'
             END, 600)
   FROM tools.uses tu
   JOIN requests.requests r ON r.id = tu.request_id
   LEFT JOIN tools.results res ON res.tool_use_id = tu.id
  WHERE r.operator_turn_id = 57589
    AND tu.session_id = 2254435
