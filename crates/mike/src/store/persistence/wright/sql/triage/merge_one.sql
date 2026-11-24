SELECT m.id, m.root_cause_id, m.absorbed_cause_id, m.problem_ids, m.by,
       m.created_at, m.separated_at
  FROM wright.cause_merges m
 WHERE m.id = $1::bigint
