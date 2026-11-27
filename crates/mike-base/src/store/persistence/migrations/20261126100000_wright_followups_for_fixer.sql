-- wright.followups_for_fixer: the filed follow-ups a fixer may take. It leaves out a code_defect
-- follow-up the repair path owns (a live or finished repair of its finding on the same part, or a
-- finding a repair run may still take) and a root_cause follow-up already answered by a child
-- follow-up of its finding. next_handoff.sql picks from it, and the check wright_followups counts
-- "issues waiting for a fixer" from it, so the check no longer reports, for good, follow-ups the
-- hand-off never takes. Each exclusion is a NOT EXISTS with the kind test inside, so the planner
-- turns all three into anti-joins instead of a SubPlan run once per follow-up.

SET LOCAL lock_timeout = '10s';

CREATE OR REPLACE VIEW wright.followups_for_fixer AS
SELECT f.id
  FROM wright.followups f
 WHERE f.state = 'filed'
   AND NOT EXISTS (
           SELECT 1
             FROM wright.followup_findings ff
             JOIN wright.repairs r ON r.finding_id = ff.finding_id
            WHERE ff.followup_id = f.id
              AND f.kind = 'code_defect'
              AND r.state NOT IN ('gate_failed', 'refused', 'dropped', 'ineffective')
              AND btrim(COALESCE(f.owning_part, r.owning_part)) = btrim(r.owning_part))
   AND NOT EXISTS (
           SELECT 1
             FROM wright.followup_findings ff
             JOIN wright.repairable_findings rf ON rf.finding_id = ff.finding_id
            WHERE ff.followup_id = f.id
              AND f.kind = 'code_defect'
              AND btrim(COALESCE(f.owning_part, rf.owning_part)) = rf.owning_part)
   AND NOT EXISTS (
           SELECT 1
             FROM wright.followup_findings root
             JOIN wright.followup_findings child
               ON child.finding_id = root.finding_id AND child.followup_id <> root.followup_id
            WHERE root.followup_id = f.id
              AND f.kind = 'root_cause');

UPDATE wright.checks
   SET query = $query$
WITH stuck AS (
    SELECT 'followup'::text AS scope, f.id::text AS resource,
           jsonb_build_object('followup_id', f.id, 'kind', f.kind, 'check_name', f.check_name,
               'title', f.title, 'issue_number', f.issue_number, 'issue_url', f.issue_url,
               'attempts', f.attempts, 'fixer_machine', f.fixer_machine,
               'closed_reason', f.closed_reason, 'updated_at', f.updated_at) AS evidence
      FROM wright.followups f
     WHERE f.state = 'stuck'
       AND f.updated_at >= now() - make_interval(days => ($1->>'window_days')::int)
     ORDER BY f.id DESC
     LIMIT 200
),
waiting AS (
    SELECT f.id, f.issue_number, f.filed_at
      FROM wright.followups f
      JOIN wright.followups_for_fixer w ON w.id = f.id
     WHERE f.filed_at < now() - make_interval(days => COALESCE(($1->>'waiting_days')::int, 3))
)
SELECT scope, resource, evidence FROM stuck
UNION ALL
SELECT 'followup', 'issues waiting for a fixer',
       jsonb_build_object('left_undone', count(*), 'oldest_filed_at', min(filed_at),
           'issues', jsonb_agg(issue_number ORDER BY filed_at),
           'last_hand_off_at', (SELECT max(handed_off_at) FROM wright.followups))
  FROM waiting
HAVING count(*) > 0
$query$,
       updated_at = now()
 WHERE name = 'wright_followups';
