UPDATE launch.conditions
   SET state = $2, fired_at = now(), outcome = $3, next_attempt_at = NULL, last_error = NULL
 WHERE id = $1 AND state = 'armed'
RETURNING id, condition_uuid, created_at, machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before, deadline, check_every_secs, last_checked_at, action, target_session, folder, brief, state, fired_at, outcome, attempts, next_attempt_at, last_error
