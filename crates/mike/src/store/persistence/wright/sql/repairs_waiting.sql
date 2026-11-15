SELECT EXISTS (
    SELECT 1
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
)
