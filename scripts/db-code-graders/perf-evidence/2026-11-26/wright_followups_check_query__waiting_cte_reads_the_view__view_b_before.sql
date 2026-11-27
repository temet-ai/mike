WITH stuck AS (
    SELECT 'followup'::text AS scope, f.id::text AS resource,
           jsonb_build_object('followup_id', f.id, 'kind', f.kind, 'check_name', f.check_name,
               'title', f.title, 'issue_number', f.issue_number, 'issue_url', f.issue_url,
               'attempts', f.attempts, 'fixer_machine', f.fixer_machine,
               'closed_reason', f.closed_reason, 'updated_at', f.updated_at) AS evidence
      FROM wright.followups f
     WHERE f.state = 'stuck'
       AND f.updated_at >= now() - make_interval(days => ('{"window_days": 7, "waiting_days": 3}'::jsonb->>'window_days')::int)
     ORDER BY f.id DESC
     LIMIT 200
),
waiting AS (
    SELECT f.id, f.issue_number, f.filed_at
      FROM wright.followups f
     WHERE f.state = 'filed'
       AND f.filed_at < now() - make_interval(days => COALESCE(('{"window_days": 7, "waiting_days": 3}'::jsonb->>'waiting_days')::int, 3))
)
SELECT scope, resource, evidence FROM stuck
UNION ALL
SELECT 'followup', 'issues waiting for a fixer',
       jsonb_build_object('left_undone', count(*), 'oldest_filed_at', min(filed_at),
           'issues', jsonb_agg(issue_number ORDER BY filed_at),
           'last_hand_off_at', (SELECT max(handed_off_at) FROM wright.followups))
  FROM waiting
HAVING count(*) > 0
