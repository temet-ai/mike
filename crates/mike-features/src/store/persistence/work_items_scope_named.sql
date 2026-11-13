WITH active AS (
    SELECT DISTINCT w.session_id
      FROM sessions.sessions s
      JOIN agents.work_items w ON w.session_id = s.id
     WHERE s.session_uuid = ANY($2::uuid[])
       AND (w.launched_at > now() - make_interval(hours => $1::int)
            OR w.state = 'running')
)
SELECT w.session_id AS "session_id!",
       s.session_uuid::text AS "session_uuid!",
       w.kind AS "kind!",
       w.task_id AS "task_id!",
       w.description AS "description?",
       w.owner_agent_id AS "owner_agent_id?",
       w.state AS "state!",
       w.launched_at AS "launched_at!",
       w.finished_at AS "finished_at?",
       w.exit_code AS "exit_code?",
       w.summary AS "summary?"
  FROM active
  JOIN sessions.sessions s ON s.id = active.session_id
  JOIN agents.work_items w ON w.session_id = active.session_id
 ORDER BY w.session_id, w.launched_at
