SELECT count(*)
  FROM wright.followups
 WHERE handed_off_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
