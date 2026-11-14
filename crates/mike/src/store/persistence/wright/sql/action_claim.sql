WITH today AS (
    SELECT count(*) AS taken
      FROM wright.actions a
     WHERE a.remedy = $1
       AND a.created_at >= date_trunc('day', now())
       AND a.state <> 'refused'
)
INSERT INTO wright.actions (remedy, problem_id, episode_id, state, by)
SELECT r.name, $2, $3, 'claimed', 'wright'
  FROM wright.remedies r, today
 WHERE r.name = $1
   AND today.taken < r.daily_limit
   AND EXISTS (
       SELECT 1 FROM wright.episodes e
        WHERE e.id = $3 AND e.problem_id = $2 AND e.closed_at IS NULL)
ON CONFLICT (remedy, episode_id) DO NOTHING
RETURNING id
