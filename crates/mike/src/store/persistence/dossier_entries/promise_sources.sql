SELECT t.id AS turn_id, t.session_id, f.stop_request_id,
       rs.completed_at AS "stop_completed_at?", t.closed_at AS "closed_at!",
       COALESCE(t.final_assistant_text, '') AS "final_text!", f.text_spans, f.session_class
  FROM memory.operator_turns t
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id AND f.ended_on = 'promise_or_wait'
  LEFT JOIN requests.requests rs ON rs.id = f.stop_request_id
 WHERE t.session_id = ANY($1)
   AND t.closed_at IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM memory.dossier_entries d
                    WHERE d.kind = 'promise' AND d.dedup_key = 'turn:' || t.id)
 ORDER BY t.session_id, t.closed_at, t.id
