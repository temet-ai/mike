SELECT e.id AS entry_id, e.request_id, e.operator_turn_id, e.kind,
       e.kind_source, e.keyword_kind, e.kind_updated_at, e.created_at
  FROM memory.ask_ledger_entries e
 WHERE e.session_id = $1
   AND e.created_at <= $2
 ORDER BY e.created_at, e.id
