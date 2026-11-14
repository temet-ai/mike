UPDATE memory.supplied_item_verdicts
   SET hand_verdict = $2, hand_checked_by = $3, hand_checked_at = now()
 WHERE supplied_item_id = $1
RETURNING supplied_item_id
