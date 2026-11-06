UPDATE steering.interventions
   SET pushed_at = NULL
 WHERE uuid = '8991ff4b-c9b5-414b-90dd-66ee6d3d9b9e' AND pushed_at = '2026-09-16 08:00:00+00' AND acked_at IS NULL
RETURNING id
