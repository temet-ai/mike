INSERT INTO steering.interventions
       (steerer_id, enrolment_id, session_id,
        verdict_id, mode_id, authority_id,
        trigger_request_id, fired_response_id,
        confidence, off_thread_topic, payload_text,
        missing_evidence, model, anchor_text, uuid, expires_at,
        verdict_json)
   SELECT 6, NULL, 670238,
          v.id, m.id, a.id,
          241225, r.id,
          0.42, NULL, 'perf check nudge text',
          NULL, 'test-model', NULL, gen_random_uuid(), now() + interval '10 minutes',
          '{"fires":false,"confidence":0.42,"reason":"perf check","nudge":"","checklist":[]}'::jsonb
     FROM steering.verdicts     v
     JOIN steering.modes        m ON m.code = 'augment'
     JOIN steering.authorities  a ON a.code = 'redirect'
     LEFT JOIN responses.responses r ON r.request_id = 241225
    WHERE v.code = 'fired'
   RETURNING id;
