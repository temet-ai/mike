WITH decided AS (
    SELECT r.id AS record_id, r.created_at,
           r.composed -> 'verdict' ->> 'problem_id' AS problem_id,
           r.composed -> 'verdict' ->> 'decision' AS decision
      FROM decisions.records r
     WHERE r.point = 'problem_solver'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.composed -> 'verdict' ->> 'problem_id' IS NOT NULL
),
followed AS (
    SELECT d.record_id, d.created_at,
           EXISTS (SELECT 1 FROM launch.fleet_events e
                    WHERE e.kind = 'problem_opened'
                      AND e.payload ->> 'problem_id' = d.problem_id
                      AND e.occurred_at > d.created_at
                      AND e.occurred_at <= d.created_at + interval '24 hours') AS reopened,
           EXISTS (SELECT 1 FROM launch.fleet_events e
                    WHERE e.kind = 'problem_escalated'
                      AND e.payload ->> 'problem_id' = d.problem_id
                      AND e.occurred_at > d.created_at
                      AND e.occurred_at <= d.created_at + interval '24 hours') AS escalated,
           d.decision
      FROM decided d
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT f.record_id,
       CASE WHEN f.reopened OR (f.escalated AND f.decision <> 'escalate')
            THEN 'wrong' ELSE 'right' END,
       'problem_after_24h',
       jsonb_build_object('reopened', f.reopened, 'escalated_after', f.escalated,
                          'decision', f.decision)
  FROM followed f
 WHERE (f.reopened OR (f.escalated AND f.decision <> 'escalate')
        OR f.created_at <= now() - interval '24 hours')
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = f.record_id AND o.label_source = 'problem_after_24h')
ON CONFLICT (record_id, label_source, label) DO NOTHING
