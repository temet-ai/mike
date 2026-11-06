SELECT session_id, pid
  FROM workspace.session_registrations
 WHERE released_at IS NULL
   AND pid IS NOT NULL
   AND boot_id = 'test-boot';
