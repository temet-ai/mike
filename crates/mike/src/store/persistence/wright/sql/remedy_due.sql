SELECT NULL::bigint AS action_id, r.name AS remedy, r.action, r.params, r.max_resources,
       p.id AS problem_id, e.id AS episode_id, p.scope, p.resource,
       NULL::text AS state, NULL::jsonb AS prior_state, NULL::jsonb AS effect
  FROM wright.remedies r
  JOIN wright.problems p ON p.check_name = r.check_name AND p.state = 'open'
  JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
 WHERE r.enabled
   AND r.paused_at IS NULL
   AND NOT EXISTS (
       SELECT 1 FROM wright.actions a WHERE a.remedy = r.name AND a.episode_id = e.id)
   AND NOT EXISTS (
       SELECT 1 FROM wright.wright_session_uuids() AS w(session_uuid)
        WHERE w.session_uuid::text = p.resource)
 ORDER BY e.opened_at, e.id
 LIMIT $1
