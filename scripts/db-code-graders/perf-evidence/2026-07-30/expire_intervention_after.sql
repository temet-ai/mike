UPDATE steering.interventions
   SET reverted_at   = NOW(),
       revert_reason = 'expired_ttl'
 WHERE uuid            = 'c5ac06e0-88b9-4335-9793-464d05043b9c'
   AND reverted_at IS NULL;
