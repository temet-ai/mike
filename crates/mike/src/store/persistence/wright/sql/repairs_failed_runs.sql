SELECT count(*)
  FROM wright.investigations
 WHERE mode = 'repair'
   AND problem_id = $1
   AND state = 'failed'
