UPDATE wright.steers
   SET undone_at = now(), version = version + 1
 WHERE id = $1 AND version = $2 AND applied_at IS NOT NULL AND undone_at IS NULL
RETURNING id, message_id, change, prior, effect, refusal, applied_at, undone_at, version
