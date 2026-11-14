UPDATE wright.reply_jobs
   SET state = 'queued', lease_until = NULL, not_before = now() + make_interval(secs => $2),
       attempts = GREATEST(attempts - $3, 0), updated_at = now()
 WHERE id = $1 AND state = 'running'
