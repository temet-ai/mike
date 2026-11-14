UPDATE wright.actions
   SET state = 'undo_conflict', refusal = $2, updated_at = now()
 WHERE id = $1
   AND state IN ('verified', 'ineffective')
RETURNING id
