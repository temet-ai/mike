SELECT ad.id AS dispatch_id, ad.agent_id AS agent_id,
       r_sub.subagent_persona
  FROM agents.dispatches ad
  JOIN tools.uses tu
    ON tu.id = ad.tool_use_id
   AND tu.tool_name = 'Agent'
  JOIN requests.requests r_tu
    ON r_tu.id = tu.request_id
  JOIN requests.turn_origins tor_sa
    ON tor_sa.code = 'subagent'
  LEFT JOIN requests.requests r_sub
    ON r_sub.session_id        = 760028
   AND r_sub.turn_origin_id    = tor_sa.id
   AND r_sub.agent_instance_id IS NULL
   AND r_sub.id = (
       SELECT rr.id FROM requests.requests rr
        WHERE rr.session_id        = 760028
          AND rr.turn_origin_id    = tor_sa.id
          AND rr.agent_instance_id IS NULL
        ORDER BY rr.id ASC
        OFFSET (
            SELECT COUNT(*)
              FROM agents.dispatches ad2
              JOIN tools.uses tu2
                ON tu2.id = ad2.tool_use_id
              JOIN requests.requests r2
                ON r2.id = tu2.request_id
             WHERE r2.session_id = 760028
               AND ad2.id < ad.id
        )
        LIMIT 1
   )
  LEFT JOIN agents.instances ai_existing
    ON ai_existing.dispatch_id = ad.id
 WHERE r_tu.session_id = 760028
   AND ai_existing.id IS NULL
