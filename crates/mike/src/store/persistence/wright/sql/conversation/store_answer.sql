WITH job AS (
    SELECT id, message_id
      FROM wright.reply_jobs
     WHERE id = $1 AND state = 'running' AND answer_id IS NULL
       FOR UPDATE
), answer AS (
    INSERT INTO wright.messages (author, body, client_msg_id, reply_to, links, needs_decision)
    SELECT 'wright', $2, 'wright-reply-' || job.id, job.message_id, $3, $4 FROM job
    ON CONFLICT (author, client_msg_id) DO NOTHING
    RETURNING id
), steer AS (
    INSERT INTO wright.steers (message_id, change, prior, refusal)
    SELECT answer.id, $5, $6, $7 FROM answer WHERE $5::jsonb IS NOT NULL
    RETURNING id
), done AS (
    UPDATE wright.reply_jobs j
       SET state = 'answered', lease_until = NULL, answer_id = answer.id, updated_at = now()
      FROM answer
     WHERE j.id = $1
    RETURNING j.id
)
SELECT answer.id AS answer_id, steer.id AS steer_id
  FROM answer LEFT JOIN steer ON true
