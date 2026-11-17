WITH waiting AS (
    SELECT q.cause_id, q.check_name, q.resource, q.problems, q.observations, q.opened_at,
           q.attempts, q.priority
      FROM wright.cause_queue q
      JOIN wright.causes c ON c.id = q.cause_id
     WHERE q.finding_id IS NULL
       AND q.followup_id IS NULL
       AND NOT q.remedied
       AND q.opened_at < now() - make_interval(hours => 6)
       AND (c.notified_at IS NULL OR c.notified_at < now() - interval '24 hours')
),
claimed AS (
    UPDATE wright.causes c
       SET notified_at = now()
      FROM waiting w
     WHERE c.id = w.cause_id
    RETURNING c.id
)
SELECT w.cause_id, w.check_name, w.resource, w.problems, w.observations, w.opened_at,
       w.attempts, w.priority
  FROM waiting w
  JOIN claimed ON claimed.id = w.cause_id
 ORDER BY w.priority DESC, w.cause_id
