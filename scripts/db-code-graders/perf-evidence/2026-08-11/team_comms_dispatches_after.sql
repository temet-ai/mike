SELECT
       COALESCE(a.name, r.subagent_persona) AS agent_label,
       COALESCE(
           (ARRAY_AGG(tu.input ->> 'description' ORDER BY tu.created_at)
                FILTER (WHERE tu.tool_name = 'Agent'
                          AND tu.input ->> 'description' IS NOT NULL)
           )[1],
           ''
       ) AS description,
       MIN(tu.created_at) AS first_seen,
       MAX(tu.created_at) AS last_seen,
       COUNT(DISTINCT tu.request_id) AS request_count
   FROM tools.uses tu
   JOIN requests.requests r ON r.id = tu.request_id
   LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
   LEFT JOIN agents.agents a ON a.id = ai.agent_id
  WHERE tu.session_id = 574726
    AND (a.name IS NOT NULL OR r.subagent_persona IS NOT NULL)
  GROUP BY COALESCE(a.name, r.subagent_persona)
  ORDER BY MIN(tu.created_at) ASC
