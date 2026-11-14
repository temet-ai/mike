WITH lim AS (
    SELECT daily_starts, wall_seconds FROM wright.engineer_limits WHERE id
),
today AS (
    SELECT count(*) AS started
      FROM wright.investigations
     WHERE created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
),
open_episode AS (
    SELECT id FROM wright.episodes WHERE problem_id = $2 AND closed_at IS NULL
)
INSERT INTO wright.investigations
       (mode, problem_id, episode_id, session_uuid, state, host, machine_uuid, pid, deadline_at)
SELECT $1, $2, (SELECT id FROM open_episode), $3, 'starting', $4, $5, $6,
       now() + make_interval(secs => lim.wall_seconds)
  FROM lim, today
 WHERE today.started < lim.daily_starts
RETURNING id, episode_id, deadline_at
