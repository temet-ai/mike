SELECT id AS "id!", role AS "role!", subject_key AS "subject_key!", origin AS "origin!",
       status AS "status!", components AS "components!", content_sha256 AS "content_sha256!",
       build_sha AS "build_sha!", build_dirty AS "build_dirty!", cause AS "cause!",
       created_at AS "created_at!", superseded_at, superseded_by_id,
       gate_run_id, promoted_at, override_reason
  FROM eval.bundles
 WHERE id = $1
