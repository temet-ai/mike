INSERT INTO launch.conditions (
    machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before,
    deadline, check_every_secs, action, target_session, folder, brief
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
RETURNING id, condition_uuid, created_at, machine_uuid, created_by_session, project_id, thread_slug, kind, spec, not_before, deadline, check_every_secs, last_checked_at, action, target_session, folder, brief, state, fired_at, outcome, attempts, next_attempt_at, last_error
