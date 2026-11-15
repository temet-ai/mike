WITH pending AS (
    SELECT i.id, i.supplied_at
      FROM memory.supplied_items i
     WHERE i.supplied_at >= $2
       AND i.supplied_at < $3
       AND NOT EXISTS (
           SELECT 1 FROM memory.supplied_item_verdicts v WHERE v.supplied_item_id = i.id)
),
ready AS (
    SELECT p.id,
           row_number() OVER (ORDER BY p.supplied_at, p.id) AS from_oldest,
           row_number() OVER (ORDER BY p.supplied_at DESC, p.id DESC) AS from_newest
      FROM pending p
      JOIN memory.supplied_items i ON i.id = p.id
      JOIN requests.requests r ON r.id = i.request_id
      JOIN memory.operator_turns ot ON ot.id = r.operator_turn_id
      LEFT JOIN memory.turn_facts f ON f.operator_turn_id = ot.id
     WHERE ot.closed_at IS NOT NULL
       AND ot.closed_at < $3
       AND (f.next_state IS NULL OR f.next_state <> 'pending')
),
chosen AS (
    SELECT id FROM ready
     WHERE from_oldest <= ($1::bigint + 1) / 2
        OR from_newest <= $1::bigint / 2
)
SELECT i.id, i.item_kind, i.route, i.item_text, i.session_id, ot.id AS turn_id,
       ot.user_prompt, ot.final_assistant_text, ot.tools_used, ot.files_touched,
       ot.reaction, nxt.human_prompt AS "next_message?"
  FROM chosen c
  JOIN memory.supplied_items i ON i.id = c.id
  JOIN requests.requests r ON r.id = i.request_id
  JOIN memory.operator_turns ot ON ot.id = r.operator_turn_id
  LEFT JOIN memory.turn_facts f ON f.operator_turn_id = ot.id
  LEFT JOIN requests.requests nxt ON nxt.id = f.next_genuine_request_id
 ORDER BY i.supplied_at, i.id
