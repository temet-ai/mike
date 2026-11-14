UPDATE wright.actions
   SET state = 'applied', effect = $2, applied_at = now(), updated_at = now()
 WHERE id = $1
   AND state = 'claimed'
