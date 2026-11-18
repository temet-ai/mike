WITH same AS (
    SELECT f.id
      FROM wright.followups f
     WHERE f.state IN ('open', 'filed', 'handed_off', 'batched', 'stuck')
       AND (f.id = $7::bigint
            OR ($1::text = 'root_cause' AND f.kind = 'root_cause' AND f.check_name = $2::text))
     ORDER BY (f.id = $7::bigint) DESC NULLS LAST, f.id
     LIMIT 1
),
inserted AS (
    INSERT INTO wright.followups (kind, check_name, owning_part, title, body)
    SELECT $1::text, $2::text, $3::text, $4::text, $5::text
     WHERE NOT EXISTS (SELECT 1 FROM same)
    ON CONFLICT (kind, COALESCE(check_name, ''), COALESCE(owning_part, ''))
        WHERE state IN ('open', 'filed', 'handed_off', 'batched', 'stuck')
    DO UPDATE SET updated_at = now()
    RETURNING id
),
target AS (
    SELECT id FROM same
    UNION ALL
    SELECT id FROM inserted
),
bumped AS (
    UPDATE wright.followups f
       SET updated_at = now()
      FROM same
     WHERE f.id = same.id
    RETURNING f.id
),
linked AS (
    INSERT INTO wright.followup_findings (followup_id, finding_id)
    SELECT id, $6::bigint FROM target
    ON CONFLICT (followup_id, finding_id) DO NOTHING
    RETURNING followup_id
)
SELECT id FROM target
