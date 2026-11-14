WITH blamed AS (
    SELECT r.id AS record_id, r.created_at, r.commit_sha AS failed_sha,
           r.composed -> 'verdict' ->> 'picked_seat' AS seat
      FROM decisions.records r
     WHERE r.point = 'gate_blame'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.commit_sha IS NOT NULL
       AND r.composed -> 'verdict' ->> 'picked_seat' IS NOT NULL
),
failed AS (
    SELECT DISTINCT ON (b.record_id) b.record_id, b.seat, ce.run_id, ce.at AS failed_at
      FROM blamed b
      JOIN workspace.change_events ce
        ON ce.kind = 'check'
       AND ce.run_id IS NOT NULL
       AND ce.after_sha = b.failed_sha
       AND ce.detail ->> 'label' = 'gate'
       AND (ce.detail ->> 'passed')::boolean = false
     ORDER BY b.record_id, abs(extract(epoch FROM ce.at - b.created_at))
),
passed AS (
    SELECT DISTINCT ON (f.record_id) f.record_id, f.seat, f.run_id, f.failed_at, ce.at AS passed_at
      FROM failed f
      JOIN workspace.change_events ce
        ON ce.kind = 'check'
       AND ce.run_id = f.run_id
       AND ce.detail ->> 'label' = 'gate'
       AND (ce.detail ->> 'passed')::boolean = true
       AND ce.at > f.failed_at
     ORDER BY f.record_id, ce.at
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT p.record_id,
       CASE WHEN EXISTS (
                SELECT 1
                  FROM launch.launches l
                  JOIN launch.fleet_events fe
                    ON fe.session_uuid = l.session_uuid
                   AND fe.run_id = p.run_id
                   AND fe.kind = 'commit_landed'
                 WHERE l.run_id = p.run_id
                   AND l.seat = p.seat
                   AND fe.occurred_at > p.failed_at
                   AND fe.occurred_at <= p.passed_at
            ) THEN 'right' ELSE 'wrong' END,
       'next_gate_pass',
       jsonb_build_object('run_id', p.run_id, 'seat', p.seat,
                          'failed_at', p.failed_at, 'passed_at', p.passed_at)
  FROM passed p
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = p.record_id AND o.label_source = 'next_gate_pass')
ON CONFLICT (record_id, label_source, label) DO NOTHING
