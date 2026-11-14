SELECT id, message_id, change, prior, effect, refusal, applied_at, undone_at, version
  FROM wright.steers
 WHERE id = $1
