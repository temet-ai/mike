SELECT q.cause_id, q.problem_id, q.check_name, q.resource, q.problems, q.observations,
       q.opened_at, q.attempts, q.last_attempt_at, q.finding_id, q.followup_id, q.issue_number,
       q.remedied, q.priority, c.notified_at, c.commented_at
  FROM wright.cause_queue q
  JOIN wright.causes c ON c.id = q.cause_id
 ORDER BY q.priority DESC, q.cause_id
 LIMIT 50
