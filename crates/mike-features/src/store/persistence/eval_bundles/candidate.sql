INSERT INTO eval.bundles
    (role, subject_key, origin, status, components, content_sha256, build_sha, build_dirty, cause)
VALUES ($1, $2, 'candidate', 'candidate', $3, $4, $5, $6, '{"trigger": "candidate"}'::jsonb)
RETURNING id AS "id!"
