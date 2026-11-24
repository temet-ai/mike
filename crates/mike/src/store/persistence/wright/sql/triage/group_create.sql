INSERT INTO wright.cause_groups (name)
VALUES ($1::text)
ON CONFLICT (name) DO NOTHING
RETURNING id, name, created_at, 0::bigint AS causes
