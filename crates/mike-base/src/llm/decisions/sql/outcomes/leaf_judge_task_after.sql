WITH judged AS (
    SELECT DISTINCT ON (r.id)
           r.id AS record_id, r.created_at, r.message_uuid,
           r.composed -> 'verdict' ->> 'verdict' AS verdict,
           rf.run_id, rf.task, rf.id AS report_event_id
      FROM decisions.records r
      JOIN launch.fleet_events rf
        ON rf.kind = 'report_filed'
       AND rf.payload ->> 'note_uuid' = r.message_uuid::text
       AND rf.run_id IS NOT NULL
       AND rf.task IS NOT NULL
     WHERE r.point = 'leaf_judge'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.message_uuid IS NOT NULL
       AND r.composed -> 'verdict' ->> 'verdict' IN ('pass', 'fail')
     ORDER BY r.id, rf.occurred_at
),
passes AS (
    SELECT j.record_id, c.id AS commit_event_id, f.state
      FROM judged j
      JOIN launch.fleets f
        ON f.run_id = j.run_id
       AND f.state IN ('done', 'failed')
      LEFT JOIN LATERAL (
            SELECT fe.id
              FROM launch.fleet_events fe
             WHERE fe.run_id = j.run_id
               AND fe.task = j.task
               AND fe.kind = 'commit_landed'
               AND fe.occurred_at > j.created_at
             ORDER BY fe.occurred_at
             LIMIT 1
      ) c ON true
     WHERE j.verdict = 'pass'
),
passed_notes AS (
    SELECT fe.run_id, fe.task, fe.payload ->> 'note_uuid' AS note_uuid,
           min(fe.id) AS verdict_event_id, max(fe.occurred_at) AS passed_at
      FROM launch.fleet_events fe
      JOIN judged j ON j.run_id = fe.run_id AND j.task = fe.task
     WHERE fe.kind = 'verdict_recorded'
       AND fe.session_uuid IS NULL
       AND fe.occurred_at > j.created_at
     GROUP BY fe.run_id, fe.task, fe.payload ->> 'note_uuid'
    HAVING bool_and((fe.payload ->> 'passed')::boolean)
),
fails AS (
    SELECT DISTINCT ON (j.record_id)
           j.record_id, n.verdict_event_id, c.id AS commit_event_id
      FROM judged j
      JOIN passed_notes n
        ON n.run_id = j.run_id
       AND n.task = j.task
       AND n.note_uuid <> j.message_uuid::text
       AND n.passed_at > j.created_at
      LEFT JOIN LATERAL (
            SELECT fe.id
              FROM launch.fleet_events fe
             WHERE fe.run_id = j.run_id
               AND fe.task = j.task
               AND fe.kind = 'commit_landed'
               AND fe.occurred_at > j.created_at
               AND fe.occurred_at < n.passed_at
             ORDER BY fe.occurred_at
             LIMIT 1
      ) c ON true
     WHERE j.verdict = 'fail'
     ORDER BY j.record_id, n.passed_at
),
labelled AS (
    SELECT p.record_id,
           CASE WHEN p.commit_event_id IS NULL THEN 'right' ELSE 'wrong' END AS label,
           jsonb_build_object('jev_verdict', 'pass', 'run_state', p.state,
                              'later_commit_event_id', p.commit_event_id) AS evidence
      FROM passes p
    UNION ALL
    SELECT f.record_id,
           'right',
           jsonb_build_object('jev_verdict', 'fail', 'passing_verdict_event_id', f.verdict_event_id,
                              'commit_event_id', f.commit_event_id)
      FROM fails f
     WHERE f.commit_event_id IS NOT NULL
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT l.record_id, l.label, 'task_after_pass', l.evidence
  FROM labelled l
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = l.record_id AND o.label_source = 'task_after_pass')
ON CONFLICT (record_id, label_source, label) DO NOTHING
