WITH let_through AS (
    SELECT r.id AS record_id,
           r.created_at,
           r.composed -> 'verdict' ->> 'task' AS task,
           f.run_id,
           f.state AS fleet_state
      FROM decisions.records r
      JOIN launch.fleets f ON f.id = r.fleet_run_id
      LEFT JOIN decisions.outcomes done
        ON done.record_id = r.id AND done.label_source = 'next_task_state'
     WHERE r.point = 'fleet_stop_check'
       AND r.applied = 'let_through'
       AND r.created_at >= now() - interval '30 days'
       AND r.composed -> 'verdict' ->> 'task' IS NOT NULL
       AND f.run_id IS NOT NULL
       AND done.record_id IS NULL
), later AS (
    SELECT l.record_id,
           l.task,
           l.fleet_state,
           COALESCE(bool_or(e.kind = 'floor_failed'
                            OR (e.kind = 'verdict_recorded' AND e.payload ->> 'passed' = 'false')),
                    false) AS failed,
           COALESCE(bool_or(e.kind = 'verdict_recorded' AND e.payload ->> 'passed' = 'true'),
                    false) AS passed
      FROM let_through l
      LEFT JOIN launch.fleet_events e
        ON e.run_id = l.run_id
       AND e.task = l.task
       AND e.kind IN ('verdict_recorded', 'floor_failed')
       AND e.occurred_at > l.created_at
     GROUP BY l.record_id, l.task, l.fleet_state
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT d.record_id,
       CASE WHEN d.failed THEN 'wrong' ELSE 'right' END,
       'next_task_state',
       jsonb_build_object('task', d.task,
                          'failed_after', d.failed,
                          'passed_after', d.passed,
                          'fleet_state', d.fleet_state)
  FROM later d
 WHERE d.failed OR d.passed OR d.fleet_state = 'done'
ON CONFLICT (record_id, label_source, label) DO NOTHING
