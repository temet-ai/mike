UPDATE wright.steers
   SET effect = $2, applied_at = now()
 WHERE id = $1 AND applied_at IS NULL AND refusal IS NULL
