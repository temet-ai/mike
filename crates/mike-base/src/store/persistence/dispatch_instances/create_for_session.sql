WITH sub AS (
    SELECT rr.id, rr.created_at, rr.subagent_persona,
           ROW_NUMBER() OVER (ORDER BY rr.id ASC) - 1 AS pos
      FROM requests.requests rr
      JOIN requests.turn_origins tor_sa
        ON tor_sa.id = rr.turn_origin_id
       AND tor_sa.code = 'subagent'
     WHERE rr.session_id = $1
       AND rr.agent_instance_id IS NULL
),
disp AS (
    SELECT ad.id AS dispatch_id, ad.agent_id, tu.tool_name,
           ROW_NUMBER() OVER (ORDER BY ad.id ASC) - 1 AS pos
      FROM agents.dispatches ad
      JOIN tools.uses tu ON tu.id = ad.tool_use_id
      JOIN requests.requests r2 ON r2.id = tu.request_id
     WHERE r2.session_id = $1
),
todo AS (
    SELECT d.dispatch_id, d.agent_id,
           CASE WHEN sub.subagent_persona ~ '\S' THEN sub.subagent_persona END AS persona
      FROM disp d
      LEFT JOIN sub ON sub.pos = d.pos
      LEFT JOIN agents.instances ai_existing
        ON ai_existing.dispatch_id = d.dispatch_id
     WHERE d.tool_name = 'Agent'
       AND ai_existing.id IS NULL
),
ranked_todo AS (
    SELECT t.dispatch_id, t.agent_id, t.persona,
           ROW_NUMBER() OVER (PARTITION BY t.persona ORDER BY t.dispatch_id ASC) AS rn
      FROM todo t
),
open_requests AS (
    SELECT s.id, s.created_at, s.subagent_persona,
           ROW_NUMBER() OVER (PARTITION BY s.subagent_persona ORDER BY s.id ASC) AS rn
      FROM sub s
     WHERE s.subagent_persona IN (SELECT persona FROM todo WHERE persona IS NOT NULL)
),
ins AS (
    INSERT INTO agents.instances
        (session_id, dispatch_id, agent_id, subagent_persona, first_request_id, started_at)
    SELECT $1, rt.dispatch_id, rt.agent_id, rt.persona, orq.id, orq.created_at
      FROM ranked_todo rt
      LEFT JOIN open_requests orq
        ON orq.subagent_persona = rt.persona
       AND orq.rn = rt.rn
     WHERE NOT EXISTS (
         SELECT 1 FROM agents.instances WHERE dispatch_id = rt.dispatch_id
     )
    RETURNING id, agent_id, first_request_id
),
claimed AS (
    UPDATE requests.requests r
       SET agent_instance_id = ins.id
      FROM ins
     WHERE r.id = ins.first_request_id
    RETURNING r.id
),
stamped AS (
    UPDATE system_prompts.session_prompts sp
       SET instance_id = ins.id,
           agent_id    = ins.agent_id
      FROM ins
     WHERE sp.request_id = ins.first_request_id
       AND sp.instance_id IS NULL
    RETURNING sp.id
)
SELECT count(*) AS "created!" FROM ins
