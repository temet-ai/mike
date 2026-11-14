WITH chk AS (
    SELECT name, source FROM wright.checks WHERE name = $1
), existing AS (
    SELECT p.id AS problem_id, e.id AS episode_id, e.last_seen_run AS run_id,
           e.evidence->>'steer_id' AS opened_by_steer
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE p.check_name = $1 AND p.scope = 'operator' AND p.resource = $2
), run AS (
    INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome, findings)
    SELECT name, source, 'event', now(), 'fail', 1
      FROM chk
     WHERE NOT EXISTS (SELECT 1 FROM existing)
    RETURNING id, check_name, source
), problem AS (
    INSERT INTO wright.problems (check_name, source, scope, resource, state, last_seen_at)
    SELECT check_name, source, 'operator', $2, 'open', now() FROM run
    ON CONFLICT (check_name, scope, resource)
    DO UPDATE SET state = 'open', healthy_at = NULL, last_seen_at = now()
    RETURNING id
), episode AS (
    INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
    SELECT problem.id, run.id, run.id,
           jsonb_build_object('opened_by', 'operator', 'steer_id', $3::bigint::text, 'resource', $2::text)
      FROM problem CROSS JOIN run
    RETURNING id, problem_id, last_seen_run
)
SELECT jsonb_build_object('problem_id', problem_id, 'episode_id', id, 'run_id', last_seen_run,
                          'opened_by_steer', $3::bigint::text)
  FROM episode
UNION ALL
SELECT jsonb_build_object('problem_id', problem_id, 'episode_id', episode_id, 'run_id', run_id,
                          'opened_by_steer', opened_by_steer)
  FROM existing
