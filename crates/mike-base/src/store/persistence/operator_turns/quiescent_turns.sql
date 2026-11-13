WITH target AS (
    SELECT * FROM unnest($1::bigint[], $2::bigint[], $3::text[])
        AS x(steerer_id, idle_after_secs, question))
SELECT s.session_uuid, s.id AS session_id, t.id AS turn_id, t.end_request_id,
       st.id AS steerer_id,
       EXTRACT(EPOCH FROM (NOW() - er.completed_at))::bigint AS "idle_seconds!",
       tg.question AS "question?"
  FROM target tg
  JOIN steering.steerers st ON st.id = tg.steerer_id AND st.enabled
  JOIN steering.enrolments e ON e.steerer_id = st.id AND e.disenrolled_at IS NULL
  JOIN memory.operator_turns t ON t.session_id = e.session_id AND t.closed_at IS NULL
  JOIN sessions.sessions s ON s.id = t.session_id
  JOIN requests.requests er ON er.id = t.end_request_id
 WHERE er.completed_at IS NOT NULL
   AND er.completed_at <= NOW() - make_interval(secs => tg.idle_after_secs)
   AND er.completed_at >= NOW() - INTERVAL '24 hours'
   AND NOT EXISTS (
       SELECT 1 FROM steering.interventions i
        WHERE i.operator_turn_id = t.id AND i.steerer_id = st.id)
   AND NOT EXISTS (
       SELECT 1 FROM steering.evaluations ev
        WHERE ev.operator_turn_id = t.id AND ev.steerer_id = st.id
          AND ev.question IS NOT DISTINCT FROM tg.question
          AND ev.trigger_point = 'quiescence')
 ORDER BY er.completed_at
 LIMIT $4
