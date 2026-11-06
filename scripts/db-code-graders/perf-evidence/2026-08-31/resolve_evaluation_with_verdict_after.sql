UPDATE steering.evaluations
   SET status = 'resolved_pass',
       intervention_uuid = NULL,
       resolved_at = NOW(),
       verdict_json = '{"fires": false, "confidence": 0.83, "reason": "the turn shows the test output it claims", "nudge": "", "checklist": []}'::jsonb,
       confidence = 0.83,
       reason = 'the turn shows the test output it claims'
 WHERE uuid = 'd6e5acc8-3d60-4abc-82b6-569b0d41d22c'
   AND status = 'dispatched';
