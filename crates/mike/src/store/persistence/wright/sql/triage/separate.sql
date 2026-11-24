WITH target AS (
    SELECT m.id, m.root_cause_id, m.absorbed_cause_id, m.problem_ids
      FROM wright.cause_merges m
     WHERE m.id = $1::bigint
       AND m.by = 'operator'
       AND m.separated_at IS NULL
       FOR UPDATE
),
roots AS (
    SELECT r.id
      FROM wright.causes r
      JOIN target t ON r.id = t.root_cause_id
    UNION
    SELECT r.merged_into
      FROM wright.causes r
      JOIN target t ON r.id = t.root_cause_id
     WHERE r.merged_into IS NOT NULL
),
released AS (
    UPDATE wright.causes c
       SET merged_into = NULL
      FROM target t
     WHERE c.id = t.absorbed_cause_id
    RETURNING c.id
),
regrafted AS (
    UPDATE wright.causes c
       SET merged_into = m.root_cause_id
      FROM target t, wright.cause_merges m
     WHERE m.root_cause_id = t.absorbed_cause_id
       AND m.separated_at IS NULL
       AND c.id = m.absorbed_cause_id
       AND c.merged_into IN (SELECT id FROM roots)
       AND NOT EXISTS (
           SELECT 1
             FROM wright.cause_merges later
            WHERE later.absorbed_cause_id = m.absorbed_cause_id
              AND later.separated_at IS NULL
              AND later.id > m.id)
    RETURNING c.id
),
restored AS (
    UPDATE wright.problems p
       SET cause_id = t.absorbed_cause_id
      FROM target t
     WHERE p.id = ANY(t.problem_ids)
       AND p.cause_id IN (SELECT id FROM roots)
    RETURNING p.id
),
done AS (
    UPDATE wright.cause_merges m
       SET separated_at = now()
      FROM target t
     WHERE m.id = t.id
    RETURNING m.id, m.root_cause_id, m.absorbed_cause_id, m.problem_ids, m.by,
              m.created_at, m.separated_at
)
SELECT d.id, d.root_cause_id, d.absorbed_cause_id, d.problem_ids, d.by,
       d.created_at, d.separated_at
  FROM done d
