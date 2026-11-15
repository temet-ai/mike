WITH lim AS (
    SELECT daily_starts, wall_seconds FROM wright.engineer_limits WHERE id
),
today AS (
    SELECT count(*) AS started
      FROM wright.investigations
     WHERE mode = 'investigate'
       AND created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
       AND (state IN ('starting', 'running', 'killed') OR tokens > 0 OR tool_calls > 0)
),
open_episode AS (
    SELECT id FROM wright.episodes WHERE problem_id = $2 AND closed_at IS NULL
)
INSERT INTO wright.investigations
       (mode, problem_id, episode_id, session_uuid, state, host, machine_uuid, pid, followup_id, deadline_at)
SELECT $1, $2, (SELECT id FROM open_episode), $3, 'starting', $4, $5, $6, $7,
       now() + make_interval(secs => lim.wall_seconds)
  FROM lim, today
 WHERE $1 <> 'investigate' OR today.started < lim.daily_starts
RETURNING id, episode_id, deadline_at
