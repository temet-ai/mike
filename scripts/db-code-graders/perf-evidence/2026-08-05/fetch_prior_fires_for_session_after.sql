SELECT off_thread_topic, created_at
  FROM steering.interventions
 WHERE session_id = 397909 AND steerer_id = 6
 ORDER BY created_at DESC
 LIMIT 10;
