WITH f AS (
    SELECT * FROM unnest($3::text[], $4::text[], $5::jsonb[]) AS f(scope, resource, evidence)
),
p AS (
    INSERT INTO wright.problems (check_name, source, scope, resource, state, last_seen_at)
    SELECT $1, $2, f.scope, f.resource, 'open', now() FROM f
    ON CONFLICT (check_name, scope, resource)
    DO UPDATE SET state = 'open', healthy_at = NULL, last_seen_at = now()
    RETURNING id, scope, resource, (xmax = 0) AS inserted
),
seen AS (
    UPDATE wright.episodes e
       SET last_seen_at = now(), last_seen_run = $6, observations = e.observations + 1,
           evidence = f.evidence
      FROM p JOIN f USING (scope, resource)
     WHERE e.problem_id = p.id AND e.closed_at IS NULL
    RETURNING e.problem_id
),
opened AS (
    INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
    SELECT p.id, $6, $6, f.evidence
      FROM p JOIN f USING (scope, resource)
     WHERE NOT EXISTS (
         SELECT 1 FROM wright.episodes e WHERE e.problem_id = p.id AND e.closed_at IS NULL)
    RETURNING problem_id
)
SELECT (SELECT count(*) FROM opened o JOIN p ON p.id = o.problem_id WHERE p.inserted) AS opened,
       (SELECT count(*) FROM opened o JOIN p ON p.id = o.problem_id WHERE NOT p.inserted) AS recurred,
       (SELECT count(*) FROM seen) AS still_open
