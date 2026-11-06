WITH cand AS (
    SELECT * FROM unnest($1::text[], $2::text[], $3::jsonb[], $4::text[])
             AS c(role, subject_key, components, content_sha256)),
cur AS (
    SELECT DISTINCT ON (b.role, b.subject_key) b.id, b.role, b.subject_key, b.components
      FROM eval.bundles b
     WHERE b.origin = 'resolver' AND b.superseded_at IS NULL
     ORDER BY b.role, b.subject_key, b.id DESC),
changed AS (
    SELECT c.* FROM cand c
      LEFT JOIN cur ON cur.role = c.role AND cur.subject_key = c.subject_key
     WHERE cur.id IS NULL OR cur.components IS DISTINCT FROM c.components),
ins AS (
    INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256,
                              build_sha, build_dirty, cause)
    SELECT role, subject_key, 'resolver', 'live_ungated', components, content_sha256, $5::text, $6::boolean, $7::jsonb
      FROM changed
    RETURNING id, role, subject_key),
superseded AS (
    UPDATE eval.bundles b
       SET superseded_at = now(), superseded_by_id = ins.id
      FROM cur JOIN ins ON ins.role = cur.role AND ins.subject_key = cur.subject_key
     WHERE b.id = cur.id
    RETURNING b.id),
orphaned AS (
    UPDATE eval.bundles b
       SET superseded_at = now()
      FROM cur
     WHERE b.id = cur.id
       AND NOT EXISTS (SELECT 1 FROM cand c WHERE c.role = cur.role AND c.subject_key = cur.subject_key)
    RETURNING b.id)
SELECT id AS "id!", role AS "role!", subject_key AS "subject_key!" FROM ins
UNION ALL
SELECT cur.id, cur.role, cur.subject_key
  FROM cur JOIN cand c ON c.role = cur.role AND c.subject_key = cur.subject_key
 WHERE cur.components IS NOT DISTINCT FROM c.components
