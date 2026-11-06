SELECT count(*)
  FROM memory.ask_ledger_entries e
  JOIN sessions.sessions s ON s.id = e.session_id
  LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id
  LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
 WHERE s.session_uuid = 'c0ca01b6-7922-4336-a6b8-4f7afa893493'
   AND (k.code IS NULL OR k.code = 'operator')
