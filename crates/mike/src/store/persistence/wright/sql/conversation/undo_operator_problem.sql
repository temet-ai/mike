WITH target AS (
    SELECT p.id AS problem_id, e.id AS episode_id
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id
     WHERE p.id = $1 AND p.state = 'open'
       AND e.id = $2 AND e.closed_at IS NULL AND e.last_seen_run = $3
       FOR UPDATE OF p, e
), removed AS (
    DELETE FROM wright.problems p
     USING target
     WHERE p.id = target.problem_id AND $4::timestamptz IS NULL
    RETURNING p.id
), closed AS (
    UPDATE wright.episodes e
       SET closed_at = now()
      FROM target
     WHERE e.id = target.episode_id AND $4::timestamptz IS NOT NULL
    RETURNING e.id
), restored AS (
    UPDATE wright.problems p
       SET state = 'healthy', healthy_at = $4
      FROM target
     WHERE p.id = target.problem_id AND $4::timestamptz IS NOT NULL
    RETURNING p.id
)
SELECT count(*) FROM target
