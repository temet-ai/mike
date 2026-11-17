UPDATE launch.conditions
   SET attempts = attempts + 1,
       last_error = $2,
       state = CASE WHEN attempts + 1 >= 4 THEN 'failed' ELSE 'armed' END,
       fired_at = NULL,
       outcome = NULL,
       next_attempt_at = CASE attempts
                             WHEN 0 THEN now() + interval '1 minute'
                             WHEN 1 THEN now() + interval '5 minutes'
                             ELSE now() + interval '30 minutes'
                         END
 WHERE id = $1 AND state IN ('armed', 'fired', 'missed')
RETURNING id, condition_uuid, created_at, machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before, deadline, check_every_secs, last_checked_at, action, target_session, folder, brief, state, fired_at, outcome, attempts, next_attempt_at, last_error
