-- Wright watches itself (Dawid, 2026-11-17): a fault in Wright's own engineer runs, repairs or
-- follow-ups opens a problem like any other, so the engineer investigates it and repair fixes
-- Wright's own code. Engineer runs the Claude API refused are left out: the limit hold covers them.
INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('wright_engineer_runs', 'capture_db', 'regular',
 'Fewer than in_a_row of Wright''s engineer runs in a row, within the last window_hours, end failed or killed without a result',
 'sql',
 $q$
WITH recent AS (
    SELECT id, mode, state, problem_id, end_reason, ended_at,
           row_number() OVER (ORDER BY id DESC) AS place
      FROM wright.investigations
     WHERE ended_at IS NOT NULL
       AND state <> 'unavailable'
       AND ended_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
),
failing AS (
    SELECT * FROM recent
     WHERE place <= ($1->>'in_a_row')::int AND state IN ('failed', 'killed')
)
SELECT 'wright_engineer'::text AS scope, 'runs_ending_without_a_result'::text AS resource,
       jsonb_build_object(
           'in_a_row', count(*),
           'runs', jsonb_agg(jsonb_build_object('id', id, 'mode', mode, 'state', state,
                   'problem_id', problem_id, 'ended_at', ended_at, 'end_reason', left(end_reason, 500))
                   ORDER BY id DESC)) AS evidence
  FROM failing
HAVING count(*) >= ($1->>'in_a_row')::int
 LIMIT 200
$q$,
 '{"window_hours": 24, "in_a_row": 3}'),
('wright_repairs', 'capture_db', 'regular',
 'No Wright repair ended gate_failed or refused in the last window_hours; each one is named with its reason and the tail of its gate output',
 'sql',
 $q$
SELECT 'repair'::text AS scope, r.id::text AS resource,
       jsonb_build_object('repair_id', r.id, 'finding_id', r.finding_id, 'problem_id', r.problem_id,
           'owning_part', r.owning_part, 'state', r.state, 'reason', left(r.reason, 2000),
           'gate_exit', r.gate_exit, 'gate_output_tail', right(r.gate_output, 4000),
           'machine_uuid', r.machine_uuid, 'updated_at', r.updated_at) AS evidence
  FROM wright.repairs r
 WHERE r.state IN ('gate_failed', 'refused')
   AND r.updated_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
 ORDER BY r.id DESC
 LIMIT 200
$q$,
 '{"window_hours": 72}'),
('wright_followups', 'capture_db', 'regular',
 'No Wright follow-up became stuck in the last window_days after its fixers failed max_attempts times',
 'sql',
 $q$
SELECT 'followup'::text AS scope, f.id::text AS resource,
       jsonb_build_object('followup_id', f.id, 'kind', f.kind, 'check_name', f.check_name,
           'title', f.title, 'issue_number', f.issue_number, 'issue_url', f.issue_url,
           'attempts', f.attempts, 'fixer_machine', f.fixer_machine, 'closed_reason', f.closed_reason,
           'updated_at', f.updated_at) AS evidence
  FROM wright.followups f
 WHERE f.state = 'stuck'
   AND f.updated_at >= now() - make_interval(days => ($1->>'window_days')::int)
 ORDER BY f.id DESC
 LIMIT 200
$q$,
 '{"window_days": 30}')
ON CONFLICT (name) DO NOTHING;
