-- wright.repairable_findings without correlated subqueries. The two NOT EXISTS tests sat under an
-- OR, so the planner kept them as a SubPlan run once per finding, in every query that reads the
-- view (repairs_claim, repairs_waiting, wright.followups_for_fixer and through it the check
-- wright_followups and the fixer hand-off). They become grouped CTEs that are left-joined in,
-- like `earlier` and `live`. Same columns and the same rows as 20261125200000_wright_own_path.sql:
--
-- * "no landed, deployed or healthy repair of the problem created after the finding" is
--   "the newest such repair is not after the finding" (or there is none);
-- * "no handed-off, batched, fixed or dropped code_defect follow-up of the finding on its part"
--   (a follow-up with no part matches every part) is "the finding's closed follow-ups carry no
--   null part and not its part".

SET LOCAL lock_timeout = '10s';

CREATE OR REPLACE VIEW wright.repairable_findings AS
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
            AND NOT COALESCE(a.any_part OR btrim(f.owning_part) = ANY (a.parts), false)));
