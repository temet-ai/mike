SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
  JOIN (SELECT f.id
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
             JOIN (
WITH earlier AS (
    SELECT finding_id, count(*) AS tries, bool_and(interrupted) AS all_interrupted
      FROM wright.repairs
     GROUP BY finding_id
),
live AS (
    SELECT DISTINCT problem_id
      FROM wright.repairs
     WHERE state IN ('editing', 'prepared', 'batched', 'landed', 'deployed')
),
landed AS (
    SELECT problem_id, max(created_at) AS last_created_at
      FROM wright.repairs
     WHERE state IN ('landed', 'deployed', 'healthy')
     GROUP BY problem_id
),
answered AS (
    SELECT ff.finding_id,
           bool_or(fu.owning_part IS NULL) AS any_part,
           array_agg(btrim(fu.owning_part)) FILTER (WHERE fu.owning_part IS NOT NULL) AS parts
      FROM wright.followup_findings ff
      JOIN wright.followups fu ON fu.id = ff.followup_id
     WHERE fu.kind = 'code_defect'
       AND fu.state IN ('handed_off', 'batched', 'fixed', 'dropped')
     GROUP BY ff.finding_id
)
SELECT f.id AS finding_id, f.problem_id, btrim(f.owning_part) AS owning_part,
       (l.problem_id IS NOT NULL) AS repair_live
  FROM wright.findings f
  JOIN wright.problems p ON p.id = f.problem_id
  LEFT JOIN earlier e ON e.finding_id = f.id
  LEFT JOIN live l ON l.problem_id = f.problem_id
  LEFT JOIN landed ld ON ld.problem_id = f.problem_id
  LEFT JOIN answered a ON a.finding_id = f.id
 WHERE f.kind = 'code_defect'
   AND f.cause_established
   AND btrim(COALESCE(f.owning_part, '')) <> ''
   AND (e.finding_id IS NULL OR (e.all_interrupted AND e.tries < 3))
   AND (p.state = 'open'
        OR (f.created_at >= now() - interval '14 days'
            AND (ld.last_created_at IS NULL OR ld.last_created_at <= f.created_at)
            AND NOT COALESCE(a.any_part OR btrim(f.owning_part) = ANY (a.parts), false)))
) rf ON rf.finding_id = ff.finding_id
            WHERE ff.followup_id = f.id
              AND f.kind = 'code_defect'
              AND btrim(COALESCE(f.owning_part, rf.owning_part)) = rf.owning_part)
   AND NOT EXISTS (
           SELECT 1
             FROM wright.followup_findings root
             JOIN wright.followup_findings child
               ON child.finding_id = root.finding_id AND child.followup_id <> root.followup_id
            WHERE root.followup_id = f.id
              AND f.kind = 'root_cause')) w ON w.id = f.id
 ORDER BY f.filed_at, f.id
 LIMIT 1
