SELECT id, message_id, change, prior, effect, refusal, applied_at, undone_at, version
  FROM wright.steers
 WHERE applied_at IS NULL AND refusal IS NULL
 ORDER BY id
 LIMIT $1
