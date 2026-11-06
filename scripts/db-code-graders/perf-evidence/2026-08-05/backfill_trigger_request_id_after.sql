UPDATE steering.interventions
   SET trigger_request_id = 240734
 WHERE uuid = 'bb4ffc05-61cc-4436-b5da-34ff16312e1d'::text::uuid
   AND trigger_request_id IS NULL;
