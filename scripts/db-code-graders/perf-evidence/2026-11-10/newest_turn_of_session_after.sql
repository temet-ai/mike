SELECT max(t.id) FROM memory.operator_turns t
   JOIN sessions.sessions s ON s.id = t.session_id
  WHERE s.session_uuid = '36106608-f114-4e6e-91ef-182745aa7165'::uuid AND (t.opened_by IS NULL OR t.opened_by = 'operator')
