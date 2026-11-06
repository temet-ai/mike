UPDATE workspace.change_events
   SET session_id = 1348628,
       kind = 'edit',
       detail = detail || '{"tool_use_id": 9}'::jsonb
 WHERE id = 249990
