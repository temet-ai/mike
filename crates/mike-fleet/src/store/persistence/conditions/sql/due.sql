SELECT id, condition_uuid, created_at, machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before, deadline, check_every_secs, last_checked_at, action, target_session, folder, brief, state, fired_at, outcome, attempts, next_attempt_at, last_error
  FROM launch.conditions
 WHERE machine_uuid = $1
   AND state = 'armed'
   AND not_before <= now()
   AND (next_attempt_at IS NULL OR next_attempt_at <= now())
 ORDER BY not_before, id
 LIMIT $2
