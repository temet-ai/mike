WITH pending AS (
    SELECT p.id, p.check_name,
           CASE WHEN c.params ->> 'group_by' = 'run'
                THEN 'run:' || e.opened_by_run
                ELSE 'resource:' || left(regexp_replace(p.resource, '-?[0-9]+|[0-9a-f]{8,}', 'N', 'g'), 60)
           END AS key
      FROM wright.problems p
      JOIN wright.checks c ON c.name = p.check_name
     CROSS JOIN LATERAL (
         SELECT ep.opened_by_run
           FROM wright.episodes ep
          WHERE ep.problem_id = p.id
          ORDER BY ep.id DESC
          LIMIT 1
     ) e
     WHERE p.state = 'open'
       AND p.cause_id IS NULL
),
keys AS (
    SELECT DISTINCT check_name, key FROM pending
),
made AS (
    INSERT INTO wright.causes (check_name, key)
    SELECT check_name, key FROM keys
    ON CONFLICT (check_name, key) DO NOTHING
    RETURNING id, check_name, key
),
known AS (
    SELECT id, check_name, key FROM made
    UNION ALL
    SELECT COALESCE(c.merged_into, c.id), c.check_name, c.key
      FROM wright.causes c
      JOIN keys k USING (check_name, key)
),
assigned AS (
    UPDATE wright.problems p
       SET cause_id = known.id
      FROM pending
      JOIN known USING (check_name, key)
     WHERE p.id = pending.id
    RETURNING p.id
)
SELECT (SELECT count(*) FROM assigned) AS assigned,
       (SELECT count(*) FROM made) AS opened
