SELECT q.problem_id, q.episode_id
  FROM wright.cause_queue q
 WHERE q.finding_id IS NULL
   AND q.followup_id IS NULL
   AND NOT q.remedied
   AND q.attempts < 2
 ORDER BY q.priority DESC, q.cause_id
 LIMIT 1
