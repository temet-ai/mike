SELECT session_id
  FROM workspace.session_registrations
 WHERE released_at IS NULL
   AND lease_expires_at < now() - interval '24 hours';
