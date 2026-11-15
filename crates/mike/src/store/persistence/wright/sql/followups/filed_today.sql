SELECT count(*)
  FROM wright.followups
 WHERE filed_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
