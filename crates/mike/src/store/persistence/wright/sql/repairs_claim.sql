WITH picked AS (
    SELECT DISTINCT ON (f.problem_id)
           f.id AS finding_id, f.problem_id, btrim(f.owning_part) AS owning_part
      FROM wright.findings f
      JOIN wright.problems p ON p.id = f.problem_id
     WHERE p.state = 'open'
       AND f.kind = 'code_defect'
       AND f.cause_established
       AND btrim(COALESCE(f.owning_part, '')) <> ''
       AND NOT EXISTS (SELECT 1 FROM wright.repairs r WHERE r.finding_id = f.id)
       AND NOT EXISTS (
           SELECT 1 FROM wright.repairs r
            WHERE r.problem_id = f.problem_id
              AND r.state IN ('editing', 'prepared', 'gating', 'landed', 'deployed'))
     ORDER BY f.problem_id, f.id
),
claimed AS (
    INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree, branch,
                                branches, machine_uuid)
    SELECT finding_id, problem_id, owning_part, 'editing',
           $1::text || '/wright-repair-' || finding_id, 'wright/repair-' || finding_id,
           ARRAY['wright/repair-' || finding_id], $2::uuid
      FROM (SELECT * FROM picked ORDER BY finding_id LIMIT $3) next
    ON CONFLICT (finding_id) DO NOTHING
    RETURNING id, finding_id, problem_id, owning_part, worktree, branch
)
SELECT c.id, c.finding_id, c.problem_id, c.owning_part, c.worktree, c.branch, p.check_name,
       f.symptom, f.confirmed_cause
  FROM claimed c
  JOIN wright.problems p ON p.id = c.problem_id
  JOIN wright.findings f ON f.id = c.finding_id
 ORDER BY c.id
