SELECT finding_id, problem_id, owning_part, repair_live FROM (
WITH earlier AS (
    SELECT finding_id, count(*) AS tries, bool_and(interrupted) AS all_interrupted
      FROM wright.repairs
     GROUP BY finding_id
),
live AS (
    SELECT DISTINCT problem_id
      FROM wright.repairs
     WHERE state IN ('editing', 'prepared', 'batched', 'landed', 'deployed')
)
SELECT f.id AS finding_id, f.problem_id, btrim(f.owning_part) AS owning_part,
       (l.problem_id IS NOT NULL) AS repair_live
  FROM wright.findings f
  JOIN wright.problems p ON p.id = f.problem_id
  LEFT JOIN earlier e ON e.finding_id = f.id
  LEFT JOIN live l ON l.problem_id = f.problem_id
 WHERE f.kind = 'code_defect'
   AND f.cause_established
   AND btrim(COALESCE(f.owning_part, '')) <> ''
   AND (e.finding_id IS NULL OR (e.all_interrupted AND e.tries < 3))
   AND (p.state = 'open'
        OR (f.created_at >= now() - interval '14 days'
            AND NOT EXISTS (
                SELECT 1
                  FROM wright.repairs r
                 WHERE r.problem_id = f.problem_id
                   AND r.created_at > f.created_at
                   AND r.state IN ('landed', 'deployed', 'healthy'))
            AND NOT EXISTS (
                SELECT 1
                  FROM wright.followup_findings ff
                  JOIN wright.followups fu ON fu.id = ff.followup_id
                 WHERE ff.finding_id = f.id
                   AND fu.kind = 'code_defect'
                   AND fu.state IN ('handed_off', 'batched', 'fixed', 'dropped')
                   AND btrim(COALESCE(fu.owning_part, f.owning_part)) = btrim(f.owning_part))))
) rf
