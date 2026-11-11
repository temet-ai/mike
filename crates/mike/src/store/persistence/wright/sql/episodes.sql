SELECT id, opened_at, last_seen_at, closed_at, opened_by_run, last_seen_run, closed_by_run,
       observations, evidence
  FROM wright.episodes
 WHERE problem_id = $1
 ORDER BY opened_at DESC, id DESC
 LIMIT $2 OFFSET $3
