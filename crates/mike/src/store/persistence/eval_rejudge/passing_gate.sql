SELECT g.id AS "id!"
   FROM eval.gates g
   JOIN eval.runs r ON r.id = g.candidate_run_id
  WHERE r.bundle_id = $1 AND g.verdict = 'pass' AND g.holdout_batch IS NOT NULL
  ORDER BY g.created_at DESC, g.id DESC
  LIMIT 1
