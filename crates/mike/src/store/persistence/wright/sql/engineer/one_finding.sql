SELECT id, investigation_id, problem_id, episode_id, symptom, suspected_cause, confirmed_cause,
       cause_established, owning_part, kind, remedy, evidence, created_at
  FROM wright.findings
 WHERE id = $1
