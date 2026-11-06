SELECT u.id AS "use_id!",
       w.state AS "state!",
       w.finished_at AS "finished_at?"
  FROM tools.uses u
  JOIN agents.work_items w ON w.wire_tool_use_id = u.wire_tool_use_id
 WHERE u.id = ANY($1)
