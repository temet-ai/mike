WITH root AS (
    SELECT COALESCE(c.merged_into, c.id) AS id
      FROM wright.problems p
      JOIN wright.causes c ON c.id = p.cause_id
     WHERE p.id = $1::bigint
),
absorbed AS (
    SELECT c.id
      FROM wright.causes c, root
     WHERE c.id <> root.id
       AND (c.id = ANY($2::bigint[]) OR c.merged_into = ANY($2::bigint[]))
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
