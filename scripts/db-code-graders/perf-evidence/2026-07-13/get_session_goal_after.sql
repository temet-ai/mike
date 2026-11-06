SELECT sg.goal, sg.next_steps, sg.role
  FROM sessions.session_goals sg
  JOIN sessions.sessions s ON s.id = sg.session_id
 WHERE s.session_uuid = '5d5b8d89-2b54-4f1e-9827-f24d8895a8d9';
