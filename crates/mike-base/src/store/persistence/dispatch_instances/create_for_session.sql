WITH open_instances AS (
    SELECT ai.id AS instance_id,
           ROW_NUMBER() OVER (ORDER BY ai.first_request_id ASC, ai.id ASC) AS rn
      FROM agents.instances ai
      JOIN requests.requests first_req ON first_req.id = ai.first_request_id
      JOIN requests.turn_origins tor ON tor.id = first_req.turn_origin_id AND tor.code = 'subagent'
     WHERE ai.session_id = $1
       AND ai.dispatch_id IS NULL
),
todo AS (
    SELECT ad.id AS dispatch_id, ad.agent_id,
           ROW_NUMBER() OVER (ORDER BY ad.id ASC) AS rn
      FROM agents.dispatches ad
      JOIN tools.uses tu ON tu.id = ad.tool_use_id AND tu.tool_name = 'Agent'
      JOIN requests.requests r2 ON r2.id = tu.request_id
     WHERE r2.session_id = $1
       AND NOT EXISTS (
           SELECT 1 FROM agents.instances ai WHERE ai.dispatch_id = ad.id
       )
),
linked AS (
    UPDATE agents.instances ai
       SET dispatch_id = t.dispatch_id,
           agent_id    = t.agent_id
      FROM todo t
      JOIN open_instances oi ON oi.rn = t.rn
     WHERE ai.id = oi.instance_id
    RETURNING ai.id, ai.agent_id, ai.first_request_id
),
ins AS (
    INSERT INTO agents.instances (session_id, dispatch_id, agent_id)
    SELECT $1, t.dispatch_id, t.agent_id
      FROM todo t
     WHERE NOT EXISTS (SELECT 1 FROM open_instances oi WHERE oi.rn = t.rn)
    RETURNING id
),
stamped AS (
    UPDATE system_prompts.session_prompts sp
       SET instance_id = l.id,
           agent_id    = l.agent_id
      FROM linked l
     WHERE sp.request_id = l.first_request_id
       AND sp.instance_id IS NULL
    RETURNING sp.id
)
SELECT (SELECT count(*) FROM linked) + (SELECT count(*) FROM ins) AS "created!"
