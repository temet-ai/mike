SELECT project_id, pid
  FROM workspace.session_registrations
 WHERE session_id = 398402
   AND released_at IS NULL;
