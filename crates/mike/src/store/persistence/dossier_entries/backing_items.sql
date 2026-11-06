SELECT w.id AS work_item_id, w.session_id, w.kind, w.task_id,
       w.owner_agent_id, w.description, w.launched_at, w.finished_at
  FROM agents.work_items w
 WHERE w.session_id = ANY($1)
 ORDER BY w.session_id, w.launched_at, w.id
