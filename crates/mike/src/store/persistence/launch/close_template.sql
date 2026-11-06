UPDATE launch.templates
   SET closed_at = now()
 WHERE name = $1 AND superseded_by IS NULL AND closed_at IS NULL
RETURNING id
