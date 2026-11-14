UPDATE wright.actions
   SET prior_state = $2, updated_at = now()
 WHERE id = $1
   AND state = 'claimed'
   AND prior_state IS NULL
