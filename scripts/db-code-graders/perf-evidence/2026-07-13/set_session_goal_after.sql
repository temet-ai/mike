INSERT INTO sessions.session_goals (session_id, goal, next_steps, set_by, role, updated_at)
SELECT s.id, 'test goal', 'test next steps', 'mcp', 'rust-developer', now()
  FROM sessions.sessions s
 WHERE s.session_uuid = '5d5b8d89-2b54-4f1e-9827-f24d8895a8d9'
ON CONFLICT (session_id) DO UPDATE
    SET goal       = excluded.goal,
        next_steps = excluded.next_steps,
        set_by     = excluded.set_by,
        role       = COALESCE(excluded.role, sessions.session_goals.role),
        updated_at = now();
