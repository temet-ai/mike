WITH earlier_tries AS (
    SELECT finding_id, count(*) AS tries, bool_and(interrupted) AS all_interrupted
      FROM wright.repairs
     GROUP BY finding_id
),
picked AS (
    SELECT DISTINCT ON (f.problem_id)
           f.id AS finding_id, f.problem_id, btrim(f.owning_part) AS owning_part,
           COALESCE(e.tries, 0) AS earlier
      FROM wright.findings f
      JOIN wright.problems p ON p.id = f.problem_id
      LEFT JOIN earlier_tries e ON e.finding_id = f.id
     WHERE p.state = 'open'
       AND f.kind = 'code_defect'
       AND f.cause_established
       AND btrim(COALESCE(f.owning_part, '')) <> ''
       AND (e.finding_id IS NULL OR (e.all_interrupted AND e.tries < 3))
       AND NOT EXISTS (
           SELECT 1 FROM wright.repairs r
            WHERE r.problem_id = f.problem_id
              AND r.state IN ('editing', 'prepared', 'gating', 'landed', 'deployed'))
     ORDER BY f.problem_id, f.id
),
named AS (
    SELECT finding_id, problem_id, owning_part,
           'wright/repair-' || finding_id
               || CASE WHEN earlier = 0 THEN '' ELSE '-r' || (earlier + 1) END AS branch
      FROM (SELECT * FROM picked ORDER BY finding_id LIMIT $3) next
),
claimed AS (
    INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree, branch,
                                branches, machine_uuid)
    SELECT finding_id, problem_id, owning_part, 'editing',
           $1::text || '/' || replace(branch, '/', '-'), branch, ARRAY[branch], $2::uuid
      FROM named
    ON CONFLICT DO NOTHING
    RETURNING id, finding_id, problem_id, owning_part, worktree, branch
)
SELECT c.id, c.finding_id, c.problem_id, c.owning_part, c.worktree, c.branch, p.check_name,
       f.symptom, f.confirmed_cause
  FROM claimed c
  JOIN wright.problems p ON p.id = c.problem_id
  JOIN wright.findings f ON f.id = c.finding_id
 ORDER BY c.id
