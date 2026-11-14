SELECT id, investigation_id, problem_id, episode_id, symptom, suspected_cause, confirmed_cause,
       cause_established, owning_part, kind, remedy, evidence, created_at
  FROM wright.findings
 WHERE ($1::bigint IS NULL OR problem_id = $1)
   AND ($2::bigint IS NULL OR id < $2)
 ORDER BY id DESC
 LIMIT $3
