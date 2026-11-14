SELECT id, opened_at, last_seen_at, closed_at, opened_by_run, last_seen_run, closed_by_run,
       observations, evidence
  FROM wright.episodes
 WHERE id = $1
