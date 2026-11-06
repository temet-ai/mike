INSERT INTO memory.lesson_maintenance_refusals AS r (pass, reason, cap)
VALUES ('placement', 'daily_budget', 400)
ON CONFLICT (pass) DO UPDATE
   SET reason = EXCLUDED.reason,
       cap = EXCLUDED.cap,
       refused_at = now(),
       logged_at = CASE WHEN r.logged_at <= now() - interval '1 hour'
                        THEN now() ELSE r.logged_at END
RETURNING (r.logged_at = now()) AS logged
