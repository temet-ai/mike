UPDATE launch.templates
   SET closed_at = now()
 WHERE name = 'verifier' AND superseded_by IS NULL AND closed_at IS NULL
RETURNING id
