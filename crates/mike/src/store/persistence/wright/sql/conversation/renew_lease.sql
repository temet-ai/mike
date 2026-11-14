UPDATE wright.reply_jobs
   SET lease_until = now() + make_interval(secs => $2), updated_at = now()
 WHERE id = $1 AND state = 'running'
