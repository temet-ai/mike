WITH old AS (
    SELECT r.name, r.enabled, r.daily_limit, r.max_resources, r.paused_at, r.pause_reason
      FROM wright.remedies r
     WHERE r.name = $1
       FOR UPDATE
), updated AS (
    UPDATE wright.remedies r
       SET enabled = coalesce($2, r.enabled),
           daily_limit = coalesce($3, r.daily_limit),
           max_resources = coalesce($4, r.max_resources),
           paused_at = CASE WHEN $2 IS TRUE THEN NULL ELSE r.paused_at END,
           pause_reason = CASE WHEN $2 IS TRUE THEN NULL ELSE r.pause_reason END,
           consecutive_refusals = CASE WHEN $2 IS TRUE THEN 0 ELSE r.consecutive_refusals END,
           ineffective_runs = CASE WHEN $2 IS TRUE THEN 0 ELSE r.ineffective_runs END,
           updated_at = now()
      FROM old
     WHERE r.name = old.name
    RETURNING r.name, r.enabled, r.daily_limit, r.max_resources, r.paused_at, r.pause_reason
), logged AS (
    INSERT INTO wright.remedy_changes (remedy, by, old_value, new_value)
    SELECT u.name, 'operator',
           jsonb_build_object('enabled', o.enabled, 'daily_limit', o.daily_limit,
                              'max_resources', o.max_resources, 'paused_at', o.paused_at,
                              'pause_reason', o.pause_reason),
           jsonb_build_object('enabled', u.enabled, 'daily_limit', u.daily_limit,
                              'max_resources', u.max_resources, 'paused_at', u.paused_at,
                              'pause_reason', u.pause_reason)
      FROM updated u
      JOIN old o ON o.name = u.name
    RETURNING remedy
)
SELECT u.name FROM updated u
