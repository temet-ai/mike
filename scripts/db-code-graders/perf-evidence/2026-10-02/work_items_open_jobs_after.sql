SELECT id, output_path
  FROM agents.work_items
 WHERE session_id = 1348628
   AND kind = 'bash'
   AND state = 'running'
   AND output_path IS NOT NULL
