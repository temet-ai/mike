WITH firing(kind, subject, detail) AS (
    SELECT * FROM unnest($3::text[], $4::text[], $5::jsonb[])
),
up AS (
    INSERT INTO eval.alarms AS a (kind, subject, detail)
    SELECT kind, subject, detail FROM firing
    ON CONFLICT (kind, subject) WHERE cleared_at IS NULL
    DO UPDATE SET last_seen_at = now(), detail = EXCLUDED.detail,
                  evaluations = a.evaluations + 1, quiet_evaluations = 0
    RETURNING a.id, a.kind, a.subject, a.first_seen_at = a.last_seen_at AS opened
),
quiet AS (
    UPDATE eval.alarms a
       SET quiet_evaluations = a.quiet_evaluations + 1,
           cleared_at = CASE WHEN a.quiet_evaluations + 1 >= $2 THEN now() END
     WHERE a.kind = ANY($1::text[]) AND a.cleared_at IS NULL
       AND NOT EXISTS (SELECT 1 FROM firing f WHERE f.kind = a.kind AND f.subject = a.subject)
    RETURNING a.id, a.kind, a.subject, a.cleared_at IS NOT NULL AS cleared
)
SELECT id AS "id!", kind AS "kind!", subject AS "subject!", opened AS "opened!", false AS "cleared!" FROM up
UNION ALL
SELECT id, kind, subject, false, cleared FROM quiet
