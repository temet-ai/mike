SELECT s.session_uuid                                        AS "session_uuid!",
       t.id                                                  AS "turn_id!",
       t.end_request_id                                      AS "end_request_id!",
       st.id                                                 AS "steerer_id!",
       EXTRACT(EPOCH FROM (NOW() - er.completed_at))::bigint AS "idle_seconds!"
  FROM memory.operator_turns t
  JOIN sessions.sessions     s  ON s.id = t.session_id
  JOIN requests.requests     er ON er.id = t.end_request_id
  JOIN steering.enrolments   e  ON e.session_id = t.session_id
  JOIN steering.steerers     st ON st.id = e.steerer_id
 WHERE t.closed_at        IS NULL
   AND er.completed_at    IS NOT NULL
   AND e.disenrolled_at   IS NULL
   AND st.kind            = 'false_done'
   AND st.enabled
   AND EXTRACT(EPOCH FROM (NOW() - er.completed_at)) >= 300::bigint
   AND NOT EXISTS (
       SELECT 1 FROM steering.interventions i
        WHERE i.operator_turn_id = t.id AND i.steerer_id = st.id
   )
 ORDER BY er.completed_at ASC
 LIMIT 50;
