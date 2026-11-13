SELECT i.id, i.item_kind, i.route, i.item_text, i.session_id, ot.id AS turn_id,
       ot.user_prompt, ot.final_assistant_text, ot.tools_used, ot.files_touched,
       ot.reaction, nxt.human_prompt AS "next_message?"
  FROM memory.supplied_items i
  JOIN requests.requests r ON r.id = i.request_id
  JOIN memory.operator_turns ot ON ot.id = r.operator_turn_id
  LEFT JOIN memory.turn_facts f ON f.operator_turn_id = ot.id
  LEFT JOIN requests.requests nxt ON nxt.id = f.next_genuine_request_id
 WHERE i.supplied_at >= $2
   AND ot.closed_at IS NOT NULL
   AND ot.closed_at < $3
   AND (f.next_state IS NULL OR f.next_state <> 'pending')
   AND NOT EXISTS (
       SELECT 1 FROM memory.supplied_item_verdicts v WHERE v.supplied_item_id = i.id)
 ORDER BY i.supplied_at, i.id
 LIMIT $1
