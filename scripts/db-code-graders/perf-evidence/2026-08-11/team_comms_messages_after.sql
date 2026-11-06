SELECT
       tu.created_at AS ts,
       CASE WHEN a.name IS NULL AND r.subagent_persona IS NULL
            THEN 'sent' ELSE 'received' END AS direction,
       CASE WHEN a.name IS NULL AND r.subagent_persona IS NULL
            THEN COALESCE(tu.input ->> 'to', '')
            ELSE COALESCE(a.name, r.subagent_persona, '')
       END AS counterpart,
       tu.input ->> 'summary' AS summary,
       COALESCE(tu.input ->> 'message', '') AS body
   FROM tools.uses tu
   JOIN requests.requests r ON r.id = tu.request_id
   LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
   LEFT JOIN agents.agents a ON a.id = ai.agent_id
  WHERE tu.session_id = 574726
    AND tu.tool_name = 'SendMessage'
    AND (
        (a.name IS NULL AND r.subagent_persona IS NULL)
        OR (tu.input ->> 'to' = 'main')
    )
  ORDER BY tu.created_at ASC
