SELECT id, kind, ref_id, state, problem_id, summary, created_at
  FROM (SELECT id, kind, ref_id, state, problem_id, summary, created_at
          FROM wright.inbox
         WHERE check_name = $1 AND read_at IS NULL
         ORDER BY id DESC
         LIMIT $2) newest
 ORDER BY id
