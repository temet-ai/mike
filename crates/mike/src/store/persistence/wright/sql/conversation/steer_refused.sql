UPDATE wright.steers
   SET refusal = $2
 WHERE id = $1 AND applied_at IS NULL AND refusal IS NULL
