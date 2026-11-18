WITH earlier AS (
    SELECT finding_id, count(*) AS tries, bool_and(interrupted) AS all_interrupted
      FROM wright.repairs
     GROUP BY finding_id
)
SELECT EXISTS (
    SELECT 1
      FROM wright.findings f
      JOIN wright.problems p ON p.id = f.problem_id
      LEFT JOIN earlier e ON e.finding_id = f.id
     WHERE p.state = 'open'
       AND f.kind = 'code_defect'
       AND f.cause_established
       AND btrim(COALESCE(f.owning_part, '')) <> ''
       AND (e.finding_id IS NULL OR (e.all_interrupted AND e.tries < 3))
       AND NOT EXISTS (
           SELECT 1 FROM wright.repairs r
            WHERE r.problem_id = f.problem_id
              AND r.state IN ('editing', 'prepared', 'gating', 'landed', 'deployed'))
)
