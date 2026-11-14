WITH triaged AS (
    SELECT r.id AS record_id, r.check_receipt_id, tc.run_id, tc.state, tc.command
      FROM decisions.records r
      JOIN launch.tree_checks tc ON tc.id = r.check_receipt_id
     WHERE r.point = 'fleet_triage'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.composed -> 'verdict' ->> 'triage' = 'retry'
),
next_check AS (
    SELECT DISTINCT ON (t.record_id) t.record_id, n.id AS next_id, n.exit_code
      FROM triaged t
      JOIN launch.tree_checks n
        ON n.run_id = t.run_id
       AND n.command = t.command
       AND n.state = t.state
       AND n.id > t.check_receipt_id
       AND n.finished_at IS NOT NULL
     ORDER BY t.record_id, n.id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT n.record_id,
       CASE WHEN n.exit_code = 0 THEN 'right' ELSE 'wrong' END,
       'next_check_same_tree',
       jsonb_build_object('next_check_id', n.next_id, 'exit_code', n.exit_code)
  FROM next_check n
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = n.record_id AND o.label_source = 'next_check_same_tree')
ON CONFLICT (record_id, label_source, label) DO NOTHING
