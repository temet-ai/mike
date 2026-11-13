-- Delayed fallback (see `proxy::remote_control::inject::spawn_push_ack_fallback`):
-- a steerer push that was emitted but never acked within the grace window is
-- released back to the request-time drain. Guarded on `pushed_at = $2` (the
-- exact instant the reservation stamped) rather than merely "still set", so
-- a second push of the same, by-then-cleared intervention is never
-- clobbered by a stale timer from the first attempt.
UPDATE steering.interventions
   SET pushed_at = NULL
 WHERE uuid = $1 AND pushed_at = $2 AND acked_at IS NULL
RETURNING id
