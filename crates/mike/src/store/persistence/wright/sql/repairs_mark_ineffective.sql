WITH back AS (
    SELECT r.id,
           CASE WHEN e.opened_at > r.deployed_at
                THEN format('the problem came back at %s, after the repair was deployed at %s',
                            e.opened_at, r.deployed_at)
                ELSE format('the problem has stayed open since the repair was deployed at %s',
                            r.deployed_at)
           END AS why
      FROM wright.repairs r
      JOIN wright.problems p ON p.id = r.problem_id AND p.state = 'open'
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE r.state = 'deployed'
       AND (e.opened_at > r.deployed_at + interval '6 hours'
            OR r.deployed_at < now() - interval '8 days')
)
UPDATE wright.repairs r
   SET state = 'ineffective', reason = back.why, updated_at = now()
  FROM back
 WHERE r.id = back.id
RETURNING r.id
