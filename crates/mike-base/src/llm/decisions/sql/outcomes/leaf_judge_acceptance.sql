WITH judged AS (
    SELECT r.id AS record_id, r.message_uuid,
           r.composed -> 'verdict' ->> 'verdict' AS verdict
      FROM decisions.records r
     WHERE r.point = 'leaf_judge'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.message_uuid IS NOT NULL
       AND r.composed -> 'verdict' ->> 'verdict' IN ('pass', 'fail')
),
acceptance AS (
    SELECT j.record_id, j.verdict,
           bool_and((e.payload ->> 'passed')::boolean) AS all_passed,
           count(*) AS criteria
      FROM judged j
      JOIN launch.fleet_events e
        ON e.kind = 'verdict_recorded'
       AND e.payload ->> 'note_uuid' = j.message_uuid::text
       AND e.session_uuid IS NOT NULL
     GROUP BY j.record_id, j.verdict
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT a.record_id,
       CASE WHEN (a.verdict = 'pass') = a.all_passed THEN 'right' ELSE 'wrong' END,
       'acceptance_verdict',
       jsonb_build_object('jev_verdict', a.verdict, 'acceptance_all_passed', a.all_passed,
                          'acceptance_criteria', a.criteria)
  FROM acceptance a
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = a.record_id AND o.label_source = 'acceptance_verdict')
ON CONFLICT (record_id, label_source, label) DO NOTHING
