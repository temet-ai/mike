SELECT r.name
  FROM wright.remedies r
 WHERE r.name = $1
   AND r.enabled
   AND r.paused_at IS NULL
   FOR UPDATE
