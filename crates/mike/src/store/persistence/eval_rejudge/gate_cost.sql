SELECT r.cost_usd / NULLIF(r.items_judged, 0) AS "mean_cost?"
   FROM eval.gates g
   JOIN eval.runs r ON r.id = g.candidate_run_id
  WHERE r.bundle_id = $1 AND g.verdict = 'pass'
  ORDER BY g.created_at DESC, g.id DESC
  LIMIT 1
