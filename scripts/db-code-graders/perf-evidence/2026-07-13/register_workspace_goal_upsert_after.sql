INSERT INTO sessions.session_goals (session_id, goal, next_steps, set_by, role, updated_at)
SELECT s.id, COALESCE('test goal', ''), COALESCE('test next steps', ''), 'hook', 'rust-developer', now()
  FROM sessions.sessions s
 WHERE s.session_uuid = '5d5b8d89-2b54-4f1e-9827-f24d8895a8d9'
ON CONFLICT (session_id) DO UPDATE
    SET goal       = CASE WHEN 'test goal'::text IS NOT NULL THEN 'test goal' ELSE sessions.session_goals.goal END,
        next_steps = CASE WHEN 'test next steps'::text IS NOT NULL THEN 'test next steps' ELSE sessions.session_goals.next_steps END,
        set_by     = excluded.set_by,
        role       = COALESCE('rust-developer', sessions.session_goals.role),
        updated_at = now();
