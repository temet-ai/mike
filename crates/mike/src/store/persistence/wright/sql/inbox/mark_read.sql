UPDATE wright.inbox b
   SET read_by = r.id, read_at = now()
  FROM wright.investigations r
  JOIN wright.problems p ON p.id = r.problem_id
 WHERE r.id = $1
   AND b.check_name = p.check_name
   AND b.read_at IS NULL
   AND b.created_at <= r.created_at
