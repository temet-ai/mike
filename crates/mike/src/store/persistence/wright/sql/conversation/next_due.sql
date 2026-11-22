SELECT min(CASE WHEN state = 'queued' THEN not_before ELSE lease_until END)
  FROM wright.reply_jobs
 WHERE state IN ('queued', 'running')
