SELECT a.id, a.remedy, r.action, a.problem_id, a.episode_id, a.state, a.by, a.prior_state,
       a.effect, a.refusal, a.effective, a.created_at, a.applied_at, a.verified_at,
       a.measured_at, a.undone_at
  FROM wright.actions a
  JOIN wright.remedies r ON r.name = a.remedy
 WHERE ($1::bigint IS NULL OR a.problem_id = $1)
   AND ($2::bigint IS NULL OR a.id < $2)
   AND ($3::bigint IS NULL OR a.id = $3)
 ORDER BY a.id DESC
 LIMIT $4
