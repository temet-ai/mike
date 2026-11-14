WITH next AS (
    SELECT id
      FROM wright.reply_jobs
     WHERE state IN ('queued', 'running')
       AND ((state = 'queued' AND not_before <= now()) OR (state = 'running' AND lease_until < now()))
     ORDER BY not_before, id
     LIMIT 1
     FOR UPDATE SKIP LOCKED
)
UPDATE wright.reply_jobs j
   SET state = 'running', attempts = j.attempts + 1,
       lease_until = now() + make_interval(secs => $1), updated_at = now()
  FROM next
 WHERE j.id = next.id
RETURNING j.id, j.message_id, j.attempts
