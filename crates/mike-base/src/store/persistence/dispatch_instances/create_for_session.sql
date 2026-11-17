WITH open_instances AS (
    SELECT ai.id AS instance_id,
           ROW_NUMBER() OVER (ORDER BY ai.first_request_id ASC, ai.id ASC) AS rn
      FROM agents.instances ai
      JOIN requests.requests first_req ON first_req.id = ai.first_request_id
      JOIN requests.turn_origins tor ON tor.id = first_req.turn_origin_id AND tor.code = 'subagent'
     WHERE ai.session_id = $1
       AND ai.dispatch_id IS NULL
),
sub AS (
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
           CASE WHEN sub.subagent_persona ~ '\S' THEN sub.subagent_persona END AS persona,
           ROW_NUMBER() OVER (ORDER BY d.dispatch_id ASC) AS rn
      FROM disp d
      LEFT JOIN sub ON sub.pos = d.pos
     WHERE d.tool_name = 'Agent'
       AND NOT EXISTS (
           SELECT 1 FROM agents.instances ai WHERE ai.dispatch_id = d.dispatch_id
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
unlinked AS (
    SELECT t.dispatch_id, t.agent_id, t.persona,
           ROW_NUMBER() OVER (PARTITION BY t.persona ORDER BY t.dispatch_id ASC) AS prn
      FROM todo t
     WHERE t.rn > (SELECT count(*) FROM open_instances)
),
open_requests AS (
    SELECT s.id, s.created_at, s.subagent_persona,
           ROW_NUMBER() OVER (PARTITION BY s.subagent_persona ORDER BY s.id ASC) AS rn
      FROM sub s
     WHERE s.subagent_persona IN (SELECT persona FROM unlinked WHERE persona IS NOT NULL)
),
ins AS (
    INSERT INTO agents.instances
        (session_id, dispatch_id, agent_id, subagent_persona, first_request_id, started_at)
    SELECT $1, u.dispatch_id, u.agent_id, u.persona, orq.id, orq.created_at
      FROM unlinked u
      LEFT JOIN open_requests orq
        ON orq.subagent_persona = u.persona
       AND orq.rn = u.prn
    RETURNING id, agent_id, first_request_id
),
claimed AS (
    UPDATE requests.requests r
       SET agent_instance_id = ins.id
      FROM ins
     WHERE r.id = ins.first_request_id
       AND r.agent_instance_id IS NULL
    RETURNING r.id
),
stamped AS (
    UPDATE system_prompts.session_prompts sp
       SET instance_id = x.id,
           agent_id    = x.agent_id
      FROM (SELECT id, agent_id, first_request_id FROM linked
            UNION ALL
            SELECT id, agent_id, first_request_id FROM ins) x
     WHERE sp.request_id = x.first_request_id
       AND sp.instance_id IS NULL
    RETURNING sp.id
)
SELECT (SELECT count(*) FROM linked) + (SELECT count(*) FROM ins) AS "created!"
