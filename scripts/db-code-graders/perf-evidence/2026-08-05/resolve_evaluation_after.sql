UPDATE steering.evaluations
   SET status = 'resolved_pass',
       intervention_uuid = NULL,
       resolved_at = NOW()
 WHERE uuid = '24c1bd09-6204-4ed1-a05a-6d27fe33d798'
   AND status = 'dispatched';
