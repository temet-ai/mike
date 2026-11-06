WITH sub AS (
    SELECT rr.id, rr.subagent_persona,
           ROW_NUMBER() OVER (ORDER BY rr.id ASC) - 1 AS pos
      FROM requests.requests rr
      JOIN requests.turn_origins tor_sa
        ON tor_sa.id = rr.turn_origin_id
       AND tor_sa.code = 'subagent'
     WHERE rr.session_id = 760028
       AND rr.agent_instance_id IS NULL
),
disp AS (
    SELECT ad.id AS dispatch_id, ad.agent_id, tu.tool_name,
           ROW_NUMBER() OVER (ORDER BY ad.id ASC) - 1 AS pos
      FROM agents.dispatches ad
      JOIN tools.uses tu ON tu.id = ad.tool_use_id
      JOIN requests.requests r2 ON r2.id = tu.request_id
     WHERE r2.session_id = 760028
)
SELECT d.dispatch_id, d.agent_id, sub.subagent_persona
  FROM disp d
  LEFT JOIN sub ON sub.pos = d.pos
  LEFT JOIN agents.instances ai_existing
    ON ai_existing.dispatch_id = d.dispatch_id
 WHERE d.tool_name = 'Agent'
   AND ai_existing.id IS NULL
