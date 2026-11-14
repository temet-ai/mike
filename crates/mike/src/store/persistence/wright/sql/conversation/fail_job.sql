WITH job AS (
    SELECT id, message_id
      FROM wright.reply_jobs
     WHERE id = $1 AND state = 'running' AND answer_id IS NULL
       FOR UPDATE
), answer AS (
    INSERT INTO wright.messages (author, body, client_msg_id, reply_to)
    SELECT 'wright', 'Wright could not answer this message: ' || $2, 'wright-reply-' || job.id, job.message_id
      FROM job
    ON CONFLICT (author, client_msg_id) DO NOTHING
    RETURNING id
)
UPDATE wright.reply_jobs j
   SET state = 'failed', error = $2, lease_until = NULL, answer_id = answer.id, updated_at = now()
  FROM job LEFT JOIN answer ON true
 WHERE j.id = job.id
RETURNING j.id
