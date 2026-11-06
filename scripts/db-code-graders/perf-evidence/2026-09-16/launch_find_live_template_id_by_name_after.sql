SELECT id, session_uuid
  FROM launch.templates
 WHERE name = 'verifier' AND superseded_by IS NULL AND closed_at IS NULL
