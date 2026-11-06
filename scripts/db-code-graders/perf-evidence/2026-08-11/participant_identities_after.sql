WITH target AS (
       SELECT s.id, s.session_uuid, s.title
         FROM sessions.sessions s
        WHERE s.session_uuid = ANY(ARRAY['5c455bcd-b4cf-45a6-945d-267f06f97bf5','1458e2ce-635c-4641-9b9b-0afee2cdf753','803d7e8c-b6df-494d-9edf-b46ceba7fbf5']::uuid[])
   ),
   subagent AS (
       SELECT DISTINCT ON (ai.session_id) ai.session_id, ag.name AS agent_type
         FROM agents.instances ai
         JOIN agents.agents ag ON ag.id = ai.agent_id
        WHERE ai.session_id IN (SELECT id FROM target)
        ORDER BY ai.session_id, ai.created_at DESC
   )
   SELECT
       target.session_uuid,
       target.title,
       sg.role,
       subagent.agent_type
     FROM target
     LEFT JOIN sessions.session_goals sg ON sg.session_id = target.id
     LEFT JOIN subagent ON subagent.session_id = target.id
