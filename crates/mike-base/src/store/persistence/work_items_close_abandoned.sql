UPDATE agents.work_items w
   SET state = 'unknown',
       finished_at = GREATEST(s.last_seen, w.launched_at),
       summary = COALESCE(w.summary, 'its session went quiet and nothing reported how it ended')
  FROM sessions.sessions s
 WHERE s.id = w.session_id
   AND w.state = 'running'
   AND s.last_seen < now() - make_interval(hours => $1::int)
