WITH problem AS (
    SELECT check_name FROM wright.problems WHERE id = $1::bigint
),
linked AS (
    SELECT ff.followup_id AS id
      FROM wright.findings fi
      JOIN wright.followup_findings ff ON ff.finding_id = fi.id
     WHERE fi.problem_id = $1::bigint
),
by_check AS (
    SELECT f.id
      FROM wright.followups f
      JOIN problem p ON p.check_name = f.check_name
     WHERE f.state IN ('open', 'filed', 'handed_off', 'stuck')
),
ids AS (
    SELECT id FROM linked
    UNION
    SELECT id FROM by_check
)
SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
  JOIN ids ON ids.id = f.id
 ORDER BY f.id DESC
 LIMIT 5
