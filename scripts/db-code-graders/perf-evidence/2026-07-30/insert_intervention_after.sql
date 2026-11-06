INSERT INTO steering.interventions
       (steerer_id, enrolment_id, session_id,
        verdict_id, mode_id, authority_id,
        trigger_request_id, fired_response_id,
        confidence, off_thread_topic, payload_text,
        missing_evidence, model, anchor_text, uuid, expires_at)
   SELECT 3, NULL, 204450,
          v.id, m.id, a.id,
          67108, r.id,
          0.9, NULL, 'nudge text',
          NULL, 'test-model', NULL, gen_random_uuid(), NOW() + interval '15 minutes'
     FROM steering.verdicts     v
     JOIN steering.modes        m ON m.code = 'augment'
     JOIN steering.authorities  a ON a.code = 'redirect'
     LEFT JOIN responses.responses r ON r.request_id = 67108
    WHERE v.code = 'fired'
   RETURNING id;
