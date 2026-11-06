SELECT kind, task_id, wire_tool_use_id
  FROM agents.work_items
 WHERE session_id = 1301433
   AND state = 'running'
   AND owner_agent_id IS NOT DISTINCT FROM NULL;
