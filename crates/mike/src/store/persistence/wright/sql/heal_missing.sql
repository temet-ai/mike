WITH healed AS (
    UPDATE wright.problems p
       SET state = 'healthy', healthy_at = now()
     WHERE p.check_name = $1
       AND p.state = 'open'
       AND (p.scope, p.resource) NOT IN (SELECT * FROM unnest($2::text[], $3::text[]))
    RETURNING p.id
),
closed AS (
    UPDATE wright.episodes e
       SET closed_at = now(), closed_by_run = $4
      FROM healed h
     WHERE e.problem_id = h.id AND e.closed_at IS NULL
    RETURNING e.problem_id
)
SELECT (SELECT count(*) FROM healed) AS "healed!"
