SELECT d.id, d.tool_use_id, u.request_id, u.created_at, u.wire_tool_use_id, u.ord,
        ag.name AS agent_name, u.input->>'description' AS description,
        i.id AS instance_id,
        CASE WHEN octet_length(i.subagent_persona) <= 128
             THEN i.subagent_persona END AS persona,
        d.worker_producer_id
 FROM agents.dispatches d JOIN tools.uses u ON u.id = d.tool_use_id
 JOIN agents.agents ag ON ag.id = d.agent_id
 LEFT JOIN agents.instances i ON i.dispatch_id = d.id
 WHERE d.id = ANY(ARRAY[2265,2264,2263,2262,2261,2260,2259,2258,2257,2256,2255,2254,2253,2252,2251,2250,2249,2248,2247,2246]) ORDER BY d.id
