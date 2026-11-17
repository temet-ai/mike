SELECT id, condition_uuid, created_at, machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before, deadline, check_every_secs, last_checked_at, action, target_session, folder, brief, state, fired_at, outcome, attempts, next_attempt_at, last_error
  FROM launch.conditions
 WHERE ($1::text IS NULL OR thread_slug = $1)
   AND ($2::text IS NULL OR state = $2)
   AND ($3::uuid IS NULL OR created_by_session = $3)
 ORDER BY id DESC
 LIMIT $4
