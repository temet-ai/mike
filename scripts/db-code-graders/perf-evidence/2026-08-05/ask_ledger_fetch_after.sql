SELECT sub.id, sub.session_id, sub.operator_turn_id, sub.kind, sub.content, sub.created_at
  FROM (
      SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at
        FROM memory.ask_ledger_entries e
        JOIN sessions.sessions s ON s.id = e.session_id
       WHERE s.session_uuid = '408c86b6-6927-4a41-bdf5-12c4744973be'
       ORDER BY e.created_at DESC
       LIMIT 50
  ) sub
  ORDER BY sub.created_at ASC
