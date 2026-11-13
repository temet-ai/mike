-- no-transaction
-- memory.seat_memory looks up a seat's report_filed and first-task events by session_uuid and kind; launch.fleet_events had no index on either.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_fleet_events_session_kind ON launch.fleet_events (session_uuid, kind);
