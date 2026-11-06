SELECT id, session_uuid
  FROM launch.templates
 WHERE name = $1 AND superseded_by IS NULL AND closed_at IS NULL
