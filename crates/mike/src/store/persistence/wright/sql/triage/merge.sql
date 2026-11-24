WITH root AS (
    SELECT COALESCE(c.merged_into, c.id) AS id
      FROM wright.problems p
      JOIN wright.causes c ON c.id = p.cause_id
     WHERE p.id = $1::bigint
),
absorbed AS (
    SELECT c.id, c.merged_into
      FROM wright.causes c, root
     WHERE c.id <> root.id
       AND (c.id = ANY($2::bigint[]) OR c.merged_into = ANY($2::bigint[]))
),
held AS (
    SELECT p.cause_id, array_agg(p.id ORDER BY p.id) AS problem_ids
      FROM wright.problems p
     WHERE p.cause_id IN (SELECT id FROM absorbed)
     GROUP BY p.cause_id
),
recorded AS (
    INSERT INTO wright.cause_merges (root_cause_id, absorbed_cause_id, problem_ids, by)
    SELECT root.id, a.id, COALESCE(h.problem_ids, '{}'::bigint[]), $3::text
      FROM absorbed a
     CROSS JOIN root
      LEFT JOIN held h ON h.cause_id = a.id
     WHERE a.id = ANY($2::bigint[])
       AND a.merged_into IS DISTINCT FROM root.id
    RETURNING id
),
marked AS (
    UPDATE wright.causes c
       SET merged_into = root.id
      FROM root
     WHERE c.id IN (SELECT id FROM absorbed)
    RETURNING c.id
),
moved AS (
    UPDATE wright.problems p
       SET cause_id = root.id
      FROM root
     WHERE p.cause_id IN (SELECT id FROM absorbed)
    RETURNING p.id
)
SELECT (SELECT count(*) FROM marked) AS causes, (SELECT count(*) FROM moved) AS problems
